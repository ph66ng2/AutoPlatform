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
) -> Result<(ProviderRequest, DocumentState, String), FiscalError> {
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
        "SELECT d.kind,d.fiscal_code,d.amount_cents,d.tax_base_cents,f.payload::text,d.fact_id,d.plan_key,d.state,COALESCE(d.operation,(SELECT CASE e.kind WHEN 'issue_confirmed' THEN 'issue' ELSE 'cancel' END FROM fiscal.document_events e WHERE e.company_id=d.company_id AND e.document_id=d.id AND e.kind IN ('issue_confirmed','cancel_confirmed') ORDER BY e.occurred_at DESC,e.id DESC LIMIT 1)) FROM fiscal.documents d JOIN fiscal.fact_snapshots f ON f.company_id=d.company_id AND f.fact_id=d.fact_id WHERE d.company_id=$1 AND d.id=$2",
        &[&session.company_id, &document_id],
    ).map_err(|_| FiscalError::Database)?.ok_or(FiscalError::NotFound)?;
    let state = DocumentState::from_db(row.get::<_, &str>(7)).ok_or(FiscalError::Database)?;
    let operation: String = row.get(8);
    let kind = DocumentKind::from_db(row.get::<_, &str>(0)).ok_or(FiscalError::Database)?;
    let fact = serde_json::from_str(row.get::<_, &str>(4)).map_err(|_| FiscalError::Database)?;
    let fact_id: Uuid = row.get(5);
    let key: &str = row.get(6);
    tx.commit().map_err(|_| FiscalError::Database)?;
    Ok((
        ProviderRequest {
            company_id: session.company_id,
            document_id,
            kind,
            fiscal_code: row.get(1),
            amount_cents: row.get(2),
            tax_base_cents: row.get(3),
            fact,
            idempotency_key: format!("{}:{fact_id}:{key}", session.company_id),
        },
        state,
        operation,
    ))
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
    let (req, state, _) = request(client, session, document_id)?;
    if state != DocumentState::Prepared {
        return Err(FiscalError::Conflict);
    }
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
        "issue",
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
    let (req, state, _) = request(client, session, document_id)?;
    if state != DocumentState::Authorized {
        return Err(FiscalError::Conflict);
    }
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
        "cancel",
        outcome,
    )
}

pub fn consult(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    provider: &impl FiscalProvider,
) -> Result<DocumentState, FiscalError> {
    let (req, state, operation) = request(client, session, document_id)?;
    if !matches!(
        state,
        DocumentState::Sending | DocumentState::CancelPending | DocumentState::Indeterminate
    ) {
        return Err(FiscalError::Conflict);
    }
    let outcome = provider.consult(&req);
    apply(client, session, document_id, state, &operation, outcome)
}

fn apply(
    client: &mut Client,
    session: Session,
    document_id: Uuid,
    from: DocumentState,
    operation: &str,
    outcome: ProviderOutcome,
) -> Result<DocumentState, FiscalError> {
    if !matches!(operation, "issue" | "cancel") {
        return Err(FiscalError::InvalidTransition);
    }
    let (state, xml): (DocumentState, Option<Vec<u8>>) = match outcome {
        ProviderOutcome::Authorized { xml } => (DocumentState::Authorized, Some(xml)),
        ProviderOutcome::Rejected if operation == "issue" => (DocumentState::Rejected, None),
        ProviderOutcome::Rejected => (DocumentState::Authorized, None),
        ProviderOutcome::Cancelled if operation == "cancel" => (DocumentState::Cancelled, None),
        ProviderOutcome::Cancelled => return Err(FiscalError::InvalidTransition),
        ProviderOutcome::Indeterminate if from == DocumentState::Indeterminate => return Ok(from),
        ProviderOutcome::Indeterminate => (DocumentState::Indeterminate, None),
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
