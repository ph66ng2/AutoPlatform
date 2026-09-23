use axum::{extract::Request, middleware::Next, response::Response};
use tracing::Instrument;
use tracing_subscriber::EnvFilter;

use ap_observability::{
    redact, CorrelationId, EventId, TenantPseudonym, HEADER_CORRELATION_ID, HEADER_EVENT_ID,
    HEADER_TENANT_KEY, HEADER_TENANT_PSEUDONYM,
};

use crate::config::{AppConfig, StartupLog};

pub async fn propagate_context(request: Request, next: Next) -> Response {
    let correlation_id = CorrelationId::from_header(
        request
            .headers()
            .get(HEADER_CORRELATION_ID)
            .and_then(|value| value.to_str().ok()),
    );
    let tenant = TenantPseudonym::from_raw(
        request
            .headers()
            .get(HEADER_TENANT_KEY)
            .and_then(|value| value.to_str().ok())
            .unwrap_or(""),
    );
    let event_id = EventId::generate();
    let method = request.method().to_string();
    let path = redact(request.uri().path());
    let span = tracing::info_span!(
        "http_request",
        correlation_id = %correlation_id,
        event_id = %event_id,
        tenant = %tenant,
        %method,
        %path,
    );
    let correlation_header = correlation_id.as_str().to_string();
    let event_header = event_id.as_str().to_string();
    let tenant_header = tenant.as_str().to_string();
    async move {
        tracing::info!("requisicao recebida");
        let mut response = next.run(request).await;
        insert_header(
            response.headers_mut(),
            HEADER_CORRELATION_ID,
            &correlation_header,
        );
        insert_header(response.headers_mut(), HEADER_EVENT_ID, &event_header);
        insert_header(
            response.headers_mut(),
            HEADER_TENANT_PSEUDONYM,
            &tenant_header,
        );
        response
    }
    .instrument(span)
    .await
}

fn insert_header(headers: &mut axum::http::HeaderMap, name: &'static str, value: &str) {
    let Ok(name) = axum::http::HeaderName::try_from(name) else {
        return;
    };
    let Ok(value) = axum::http::HeaderValue::from_str(value) else {
        return;
    };
    headers.insert(name, value);
}

pub(crate) fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .init();
}

pub(crate) fn log_startup(config: &AppConfig, modules: &[&str]) {
    let startup = StartupLog::from_config(config);
    let module_count = modules.len();
    let modules = modules.join(",");
    tracing::info!(
        service = %startup.service,
        environment = %startup.environment,
        bind = %startup.bind,
        run_migrations_on_startup = startup.run_migrations_on_startup,
        database_configured = config.database_configured(),
        module_count,
        %modules,
        "processo iniciado"
    );
}
