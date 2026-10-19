//! Infraestrutura de processo: configuração, health e servidor HTTP.
//!
//! Não contém tabela de negócio nem provider. Segredos opcionais permanecem
//! redigidos em `Debug`, `Display` e `Serialize`.

mod config;
mod health;
mod serve;
mod telemetry;

pub use config::{load, AppConfig, ConfigError, Environment, ServiceKind};
pub use serve::{serve, serve_with, serve_with_readiness};
pub use telemetry::record_verified_tenant;
