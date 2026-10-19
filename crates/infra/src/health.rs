use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;

use crate::config::{AppConfig, Environment, ServiceKind};

pub(crate) type ReadinessProbe = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
pub(crate) struct AppState {
    service: ServiceKind,
    environment: Environment,
    readiness: Option<ReadinessProbe>,
}

impl AppState {
    pub(crate) fn from_config(config: &AppConfig) -> Self {
        Self {
            service: config.service,
            environment: config.environment,
            readiness: None,
        }
    }

    pub(crate) fn with_readiness(mut self, probe: ReadinessProbe) -> Self {
        self.readiness = Some(probe);
        self
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

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyBody<'static>>) {
    let healthy = if let Some(probe) = state.readiness {
        tokio::task::spawn_blocking(move || probe())
            .await
            .unwrap_or(false)
    } else {
        true
    };
    (
        if healthy {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadyBody {
            status: if healthy { "ready" } else { "unavailable" },
            service: state.service.as_str(),
            environment: state.environment.as_str(),
            migrations_on_startup: false,
        }),
    )
}
