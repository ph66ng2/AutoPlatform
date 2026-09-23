use std::{future::Future, io};

use thiserror::Error;
use tokio::net::TcpListener;

use crate::{
    config::AppConfig,
    health::{router, AppState},
    telemetry::{init_tracing, log_startup},
};

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("não foi possível escutar em {bind}")]
    Bind {
        bind: std::net::SocketAddr,
        #[source]
        source: io::Error,
    },
    #[error("servidor HTTP encerrou com erro")]
    Http(#[source] io::Error),
}

pub async fn serve(config: AppConfig, modules: &'static [&'static str]) -> Result<(), ServeError> {
    init_tracing();
    log_startup(&config, modules);
    let listener = TcpListener::bind(config.http_bind)
        .await
        .map_err(|source| ServeError::Bind {
            bind: config.http_bind,
            source,
        })?;
    serve_listener(listener, AppState::from_config(&config), shutdown_signal()).await
}

pub(crate) async fn serve_listener(
    listener: TcpListener,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), ServeError> {
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(ServeError::Http)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::Value;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        time::timeout,
    };

    use super::serve_listener;
    use crate::{
        config::{load_from, ServiceKind, ENV_DATABASE_URL},
        health::AppState,
    };

    async fn http_get(addr: std::net::SocketAddr, path: &str) -> (u16, String) {
        let mut stream = timeout(Duration::from_secs(2), TcpStream::connect(addr))
            .await
            .expect("timeout de conexão")
            .expect("conexão");
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("escrita");
        let mut response = String::new();
        timeout(Duration::from_secs(2), stream.read_to_string(&mut response))
            .await
            .expect("timeout de leitura")
            .expect("leitura");
        let (head, body) = response.split_once("\r\n\r\n").expect("cabeçalho HTTP");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("status");
        (status, body.to_string())
    }

    #[tokio::test]
    async fn health_and_ready_work_without_leaking_secrets() {
        let password = "super-secret-token";
        let secret = ["postgres://", "user:", password, "@db/app"].concat();
        for service in [ServiceKind::Api, ServiceKind::Worker] {
            let config = load_from(service, |key| {
                Ok(if key == ENV_DATABASE_URL {
                    Some(secret.to_string())
                } else {
                    None
                })
            })
            .expect("config");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let addr = listener.local_addr().expect("addr");
            let (shutdown, stop) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(serve_listener(
                listener,
                AppState::from_config(&config),
                async move {
                    stop.await.ok();
                },
            ));

            let health = http_get(addr, "/health").await;
            let ready = http_get(addr, "/ready").await;
            let missing = http_get(addr, "/fiscal").await;
            let _ = shutdown.send(());
            server.await.expect("task").expect("server");

            let health_json: Value = serde_json::from_str(&health.1).expect("health json");
            let ready_json: Value = serde_json::from_str(&ready.1).expect("ready json");
            assert_eq!(health.0, 200);
            assert_eq!(health_json["status"], "ok");
            assert_eq!(health_json["service"], service.as_str());
            assert_eq!(ready.0, 200);
            assert_eq!(ready_json["status"], "ready");
            assert_eq!(ready_json["environment"], "local");
            assert_eq!(ready_json["migrations_on_startup"], false);
            assert_eq!(missing.0, 404);
            assert!(!health.1.contains("super-secret-token"));
            assert!(!ready.1.contains("super-secret-token"));
        }
    }

    #[tokio::test]
    async fn correlation_headers_hide_the_tenant_key() {
        use ap_observability::TenantPseudonym;

        let raw_tenant = "tenant-raw-synthetic";
        let secret = ["super", "-secret-token"].concat();
        let expected = TenantPseudonym::from_raw(raw_tenant);
        for service in [ServiceKind::Api, ServiceKind::Worker] {
            let config = load_from(service, |_| Ok(None)).expect("config");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let addr = listener.local_addr().expect("addr");
            let (shutdown, stop) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(serve_listener(
                listener,
                AppState::from_config(&config),
                async move {
                    stop.await.ok();
                },
            ));

            let headers =
                format!("x-correlation-id: trace-synthetic-0001\r\nx-tenant-key: {raw_tenant}\r\n");
            let health = http_exchange(addr, "/health", &headers).await;
            let leaked = http_exchange(
                addr,
                "/health",
                &format!("x-correlation-id: {secret}\r\nx-tenant-key: {secret}\r\n"),
            )
            .await;
            let _ = shutdown.send(());
            server.await.expect("task").expect("server");

            assert_eq!(health.0, 200);
            assert!(health
                .1
                .to_ascii_lowercase()
                .contains("x-correlation-id: trace-synthetic-0001"));
            assert!(health
                .1
                .to_ascii_lowercase()
                .contains(&format!("x-tenant-pseudonym: {}", expected.as_str())));
            assert!(!health.1.contains(raw_tenant));
            assert!(!health.2.contains(raw_tenant));
            assert!(!leaked.1.contains(&secret));
            assert!(!leaked.2.contains(&secret));
        }
    }

    async fn http_exchange(
        addr: std::net::SocketAddr,
        path: &str,
        headers: &str,
    ) -> (u16, String, String) {
        let mut stream = timeout(Duration::from_secs(2), TcpStream::connect(addr))
            .await
            .expect("timeout de conexão")
            .expect("conexão");
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{headers}Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("escrita");
        let mut response = String::new();
        timeout(Duration::from_secs(2), stream.read_to_string(&mut response))
            .await
            .expect("timeout de leitura")
            .expect("leitura");
        let (head, body) = response.split_once("\r\n\r\n").expect("cabeçalho HTTP");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("status");
        (status, head.to_string(), body.to_string())
    }
}
