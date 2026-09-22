use std::{fmt, net::SocketAddr};

use serde::Serialize;
use thiserror::Error;

pub const ENV_APP: &str = "APP_ENV";
pub const ENV_HTTP_BIND: &str = "HTTP_BIND";
pub const ENV_MIGRATIONS_ON_STARTUP: &str = "RUN_MIGRATIONS_ON_STARTUP";
pub const ENV_DATABASE_URL: &str = "DATABASE_URL";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Api,
    Worker,
}

impl ServiceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Worker => "worker",
        }
    }
}

impl fmt::Display for ServiceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Local,
    Staging,
    Production,
}

impl Environment {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Valor sensível que não pode aparecer em log, erro ou JSON.
///
/// O conteúdo fica retido para um ticket futuro usar em memória. Nesta fase
/// só testes leem o valor; `Debug`, `Display` e `Serialize` o ocultam.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    #[must_use]
    pub const fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub(crate) fn is_present(&self) -> bool {
        !self.0.is_empty()
    }

    /// Valor em memória. Não enviar para log, erro, trace ou resposta.
    #[cfg(test)]
    #[must_use]
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("[REDACTED]")
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub(crate) service: ServiceKind,
    pub(crate) environment: Environment,
    pub(crate) http_bind: SocketAddr,
    database_url: Option<SecretString>,
}

impl AppConfig {
    #[must_use]
    pub(crate) fn database_configured(&self) -> bool {
        self.database_url
            .as_ref()
            .is_some_and(SecretString::is_present)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupLog {
    pub service: ServiceKind,
    pub environment: Environment,
    pub bind: SocketAddr,
    pub run_migrations_on_startup: bool,
}

impl StartupLog {
    pub(crate) fn from_config(config: &AppConfig) -> Self {
        Self {
            service: config.service,
            environment: config.environment,
            bind: config.http_bind,
            run_migrations_on_startup: false,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("APP_ENV desconhecido: {0}")]
    UnknownEnvironment(String),
    #[error("HTTP_BIND inválido: {0}")]
    InvalidBind(String),
    #[error(
        "RUN_MIGRATIONS_ON_STARTUP não pode ser verdadeiro: migrations não rodam no startup; produção exige job explícito"
    )]
    MigrationsOnStartupForbidden,
    #[error("valor inválido para {name}: {value}")]
    InvalidBool { name: &'static str, value: String },
    #[error("variável {0} não está em unicode")]
    NotUnicode(String),
}

pub fn load(service: ServiceKind) -> Result<AppConfig, ConfigError> {
    load_from(service, read_env)
}

pub(crate) fn load_from(
    service: ServiceKind,
    env: impl Fn(&str) -> Result<Option<String>, ConfigError>,
) -> Result<AppConfig, ConfigError> {
    let environment = parse_environment(env(ENV_APP)?)?;
    let http_bind = parse_bind(service, env(ENV_HTTP_BIND)?)?;
    if parse_bool(ENV_MIGRATIONS_ON_STARTUP, env(ENV_MIGRATIONS_ON_STARTUP)?)? {
        return Err(ConfigError::MigrationsOnStartupForbidden);
    }
    Ok(AppConfig {
        service,
        environment,
        http_bind,
        database_url: parse_secret(env(ENV_DATABASE_URL)?),
    })
}

fn read_env(key: &str) -> Result<Option<String>, ConfigError> {
    match std::env::var(key) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(ConfigError::NotUnicode(key.to_string())),
    }
}

fn parse_environment(value: Option<String>) -> Result<Environment, ConfigError> {
    let raw = value.unwrap_or_else(|| "local".to_string());
    let trimmed = raw.trim();
    let normalized = if trimmed.is_empty() {
        "local".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    };
    match normalized.as_str() {
        "local" => Ok(Environment::Local),
        "staging" => Ok(Environment::Staging),
        "production" => Ok(Environment::Production),
        other => Err(ConfigError::UnknownEnvironment(other.to_string())),
    }
}

fn parse_bind(service: ServiceKind, value: Option<String>) -> Result<SocketAddr, ConfigError> {
    let default = match service {
        ServiceKind::Api => "127.0.0.1:8080",
        ServiceKind::Worker => "127.0.0.1:8081",
    };
    let raw = value.unwrap_or_default();
    let trimmed = raw.trim();
    let candidate = if trimmed.is_empty() { default } else { trimmed };
    candidate
        .parse()
        .map_err(|_| ConfigError::InvalidBind(candidate.to_string()))
}

fn parse_bool(name: &'static str, value: Option<String>) -> Result<bool, ConfigError> {
    let Some(raw) = value else {
        return Ok(false);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("false") {
        Ok(false)
    } else if trimmed.eq_ignore_ascii_case("true") {
        Ok(true)
    } else {
        Err(ConfigError::InvalidBool {
            name,
            value: trimmed.to_string(),
        })
    }
}

fn parse_secret(value: Option<String>) -> Option<SecretString> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(SecretString::new(trimmed.to_string()))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        load_from, ConfigError, Environment, ServiceKind, ENV_APP, ENV_DATABASE_URL, ENV_HTTP_BIND,
        ENV_MIGRATIONS_ON_STARTUP,
    };

    fn env<'a>(
        vars: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Result<Option<String>, ConfigError> + 'a {
        move |key| {
            Ok(vars
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string()))
        }
    }

    #[test]
    fn defaults_work_for_every_service_without_secrets() {
        for service in [ServiceKind::Api, ServiceKind::Worker] {
            let config = load_from(service, env(&[])).expect("config");
            assert_eq!(config.environment, Environment::Local);
            assert!(!config.database_configured());
            let expected = match service {
                ServiceKind::Api => "127.0.0.1:8080",
                ServiceKind::Worker => "127.0.0.1:8081",
            };
            assert_eq!(config.http_bind, expected.parse().unwrap());
        }
    }

    #[test]
    fn staging_and_production_do_not_require_secrets() {
        for name in ["staging", "production"] {
            let config = load_from(ServiceKind::Api, env(&[(ENV_APP, name)])).expect("config");
            assert_eq!(config.environment.as_str(), name);
            assert!(!config.database_configured());
        }
    }

    #[test]
    fn rejects_startup_migrations_in_every_environment() {
        for name in ["local", "staging", "production"] {
            let error = load_from(
                ServiceKind::Api,
                env(&[(ENV_APP, name), (ENV_MIGRATIONS_ON_STARTUP, "true")]),
            )
            .expect_err("migrations");
            assert_eq!(error, ConfigError::MigrationsOnStartupForbidden);
        }
    }

    #[test]
    fn rejects_unknown_environment_and_invalid_bind() {
        let env_error = load_from(ServiceKind::Api, env(&[(ENV_APP, "qa")])).expect_err("env");
        assert_eq!(env_error, ConfigError::UnknownEnvironment("qa".to_string()));
        let bind_error = load_from(ServiceKind::Worker, env(&[(ENV_HTTP_BIND, "not-a-socket")]))
            .expect_err("bind");
        assert_eq!(
            bind_error,
            ConfigError::InvalidBind("not-a-socket".to_string())
        );
    }

    #[test]
    fn debug_display_and_json_redact_database_url() {
        let password = "super-secret-token";
        let secret = ["postgres://", "user:", password, "@db/app"].concat();
        let config = load_from(
            ServiceKind::Api,
            env(&[(ENV_DATABASE_URL, secret.as_str())]),
        )
        .expect("config");
        let stored = config.database_url.as_ref().expect("secret");
        assert!(config.database_configured());
        assert_eq!(stored.expose(), secret);
        let rendered = format!("{config:?} {stored} {stored:?}");
        assert!(!rendered.contains("super-secret-token"));
        assert!(rendered.contains("[REDACTED]"));
        let json = serde_json::to_string(stored).expect("json");
        assert_eq!(json, "\"[REDACTED]\"");
        let startup = super::StartupLog::from_config(&config);
        assert!(!startup.run_migrations_on_startup);
        assert!(!format!("{startup:?}").contains("super-secret-token"));
    }
}
