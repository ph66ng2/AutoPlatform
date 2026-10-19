#[path = "../../wired_modules.rs"]
mod modules;

use ap_identity::{authenticate, JwtSecret};
use ap_infra::{load, serve_with, Environment, ServiceKind};
use ap_integration::{find, find_by_correlation, ingest, IngestError};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone)]
struct ApiState {
    database_url: String,
    jwt_secret: String,
}

#[derive(Serialize)]
struct ReceiptBody {
    fact_id: String,
    created: bool,
}

#[derive(Deserialize)]
struct CorrelationQuery {
    correlation_id: String,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = load(ServiceKind::Api)?;
    let jwt_secret = std::env::var("JWT_SECRET")
        .ok()
        .filter(|value| !value.is_empty());
    let router = if let (Some(database_url), Some(jwt_secret)) = (config.database_url(), jwt_secret)
    {
        let state = ApiState {
            database_url: database_url.to_owned(),
            jwt_secret,
        };
        Router::new()
            .route(
                "/v2/commercial-facts",
                post(post_fact).get(get_by_correlation),
            )
            .route("/v2/commercial-facts/:fact_id", get(get_fact))
            .with_state(state)
    } else if config.environment() == Environment::Local {
        Router::new()
    } else {
        return Err("DATABASE_URL e JWT_SECRET são necessários para a API".into());
    };
    serve_with(config, modules::NAMES, router).await?;
    Ok(())
}

fn auth(headers: &HeaderMap, state: &ApiState) -> Result<(Uuid, Uuid), StatusCode> {
    let company = headers
        .get("x-company-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let account = authenticate(&JwtSecret::new(state.jwt_secret.as_bytes()), token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok((company, account))
}

fn status(error: IngestError) -> StatusCode {
    match error {
        IngestError::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
        IngestError::Denied => StatusCode::FORBIDDEN,
        IngestError::NotFound => StatusCode::NOT_FOUND,
        IngestError::Conflict => StatusCode::CONFLICT,
        IngestError::Database => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn post_fact(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(fact): Json<Value>,
) -> Result<(StatusCode, Json<ReceiptBody>), StatusCode> {
    let (company, account) = auth(&headers, &state)?;
    let receipt = tokio::task::spawn_blocking(move || {
        let mut client = postgres::Client::connect(&state.database_url, postgres::NoTls)
            .map_err(|_| IngestError::Database)?;
        ingest(&mut client, company, account, &fact)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    .map_err(status)?;
    Ok((
        if receipt.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(ReceiptBody {
            fact_id: receipt.fact_id.to_string(),
            created: receipt.created,
        }),
    ))
}

async fn get_fact(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(fact_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let (company, account) = auth(&headers, &state)?;
    let fact_id = Uuid::parse_str(&fact_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let fact = tokio::task::spawn_blocking(move || {
        let mut client = postgres::Client::connect(&state.database_url, postgres::NoTls)
            .map_err(|_| IngestError::Database)?;
        find(&mut client, company, account, fact_id)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    .map_err(status)?;
    Ok(Json(fact))
}

async fn get_by_correlation(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<CorrelationQuery>,
) -> Result<Json<Vec<Value>>, StatusCode> {
    if query.correlation_id.len() < 16 || query.correlation_id.len() > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (company, account) = auth(&headers, &state)?;
    let facts = tokio::task::spawn_blocking(move || {
        let mut client = postgres::Client::connect(&state.database_url, postgres::NoTls)
            .map_err(|_| IngestError::Database)?;
        find_by_correlation(&mut client, company, account, &query.correlation_id)
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    .map_err(status)?;
    Ok(Json(facts))
}
