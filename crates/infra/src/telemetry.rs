use tracing_subscriber::EnvFilter;

use crate::config::{AppConfig, StartupLog};

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
