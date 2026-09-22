use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::config::{AppConfig, Environment, ServiceKind};

#[derive(Clone, Copy)]
pub(crate) struct AppState {
    service: ServiceKind,
    environment: Environment,
}

impl AppState {
    pub(crate) fn from_config(config: &AppConfig) -> Self {
        Self {
            service: config.service,
            environment: config.environment,
        }
    }
}

#[derive(Serialize)]
struct HealthBody<'a> {
    status: &'a str,
    service: &'a str,
}

#[derive(Serialize)]
struct ReadyBody<'a> {
    status: &'a str,
    service: &'a str,
    environment: &'a str,
    migrations_on_startup: bool,
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthBody<'static>> {
    Json(HealthBody {
        status: "ok",
        service: state.service.as_str(),
    })
}

async fn ready(State(state): State<AppState>) -> Json<ReadyBody<'static>> {
    Json(ReadyBody {
        status: "ready",
        service: state.service.as_str(),
        environment: state.environment.as_str(),
        migrations_on_startup: false,
    })
}
