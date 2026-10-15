use ap_identity::{Permission, Session};
use postgres::Client;
use uuid::Uuid;

use crate::model::{
    DocumentKind, DocumentState, FiscalError, FiscalProvider, HumanConfirmation, ProviderOutcome,
    ProviderRequest,
};
use crate::store::transition;

fn request(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    expected: DocumentState,
) -> Result<ProviderRequest, FiscalError> {
    if !session.allows(Permission::Fiscal) {
        return Err(FiscalError::Denied);
    }
    let mut tx = client.transaction().map_err(|_| FiscalError::Database)?;
    tx.execute("SET LOCAL ROLE fiscal_app", &[])
        .map_err(|_| FiscalError::Database)?;
    tx.execute(
        "SELECT set_config('app.company_id',$1,true),set_config('app.account_id',$2,true)",
        &[
            &session.company_id.to_string(),
            &session.account_id.to_string(),
        ],
    )
    .map_err(|_| FiscalError::Database)?;
    let row = tx.query_opt(
        "SELECT d.kind,d.fiscal_code,d.amount_cents,d.tax_base_cents,f.payload::text,d.fact_id,d.plan_key,d.state FROM fiscal.documents d JOIN fiscal.fact_snapshots f ON f.company_id=d.company_id AND f.fact_id=d.fact_id WHERE d.company_id=$1 AND d.id=$2",
        &[&session.company_id, &document_id],
    ).map_err(|_| FiscalError::Database)?.ok_or(FiscalError::NotFound)?;
    if row.get::<_, &str>(7) != expected.as_str() {
        return Err(FiscalError::Conflict);
    }
    let kind = DocumentKind::from_db(row.get::<_, &str>(0)).ok_or(FiscalError::Database)?;
    let fact = serde_json::from_str(row.get::<_, &str>(4)).map_err(|_| FiscalError::Database)?;
    let fact_id: Uuid = row.get(5);
    let key: &str = row.get(6);
    tx.commit().map_err(|_| FiscalError::Database)?;
    Ok(ProviderRequest {
        company_id: session.company_id,
        document_id,
        kind,
        fiscal_code: row.get(1),
        amount_cents: row.get(2),
        tax_base_cents: row.get(3),
        fact,
        idempotency_key: format!("{}:{fact_id}:{key}", session.company_id),
    })
}

/// A chamada remota acontece depois da transição para sending. Se a resposta
/// for incerta, o documento fica indeterminate e só consult() pode concluí-lo.
pub fn issue(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    confirmation: HumanConfirmation,
    provider: &impl FiscalProvider,
) -> Result<DocumentState, FiscalError> {
    let req = request(client, session, document_id, DocumentState::Prepared)?;
    transition(
        client,
        session,
        document_id,
        DocumentState::Prepared,
        DocumentState::Sending,
        Some(confirmation),
        None,
    )?;
    let outcome = provider.issue(&req);
    apply(
        client,
        session,
        document_id,
        DocumentState::Sending,
        outcome,
    )
}

pub fn cancel(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    confirmation: HumanConfirmation,
    provider: &impl FiscalProvider,
) -> Result<DocumentState, FiscalError> {
    let req = request(client, session, document_id, DocumentState::Authorized)?;
    transition(
        client,
        session,
        document_id,
        DocumentState::Authorized,
        DocumentState::CancelPending,
        Some(confirmation),
        None,
    )?;
    let outcome = provider.cancel(&req);
    apply(
        client,
        session,
        document_id,
        DocumentState::CancelPending,
        outcome,
    )
}

pub fn consult(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    provider: &impl FiscalProvider,
) -> Result<DocumentState, FiscalError> {
    let req = request(client, session, document_id, DocumentState::Indeterminate)?;
    let outcome = provider.consult(&req);
    apply(
        client,
        session,
        document_id,
        DocumentState::Indeterminate,
        outcome,
    )
}

fn apply(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    from: DocumentState,
    outcome: ProviderOutcome,
) -> Result<DocumentState, FiscalError> {
    let (state, xml): (DocumentState, Option<Vec<u8>>) = match (from, outcome) {
        (_, ProviderOutcome::Authorized { xml }) => (DocumentState::Authorized, Some(xml)),
        (DocumentState::Sending | DocumentState::Indeterminate, ProviderOutcome::Rejected) => {
            (DocumentState::Rejected, None)
        }
        (DocumentState::CancelPending, ProviderOutcome::Rejected) => {
            (DocumentState::Authorized, None)
        }
        (
            DocumentState::CancelPending | DocumentState::Indeterminate,
            ProviderOutcome::Cancelled,
        ) => (DocumentState::Cancelled, None),
        (DocumentState::Sending | DocumentState::CancelPending, ProviderOutcome::Indeterminate) => {
            (DocumentState::Indeterminate, None)
        }
        (DocumentState::Indeterminate, ProviderOutcome::Indeterminate) => {
            return Ok(DocumentState::Indeterminate)
        }
        _ => return Err(FiscalError::InvalidTransition),
    };
    transition(
        client,
        session,
        document_id,
        from,
        state,
        None,
        xml.as_deref(),
    )?;
    Ok(state)
}
