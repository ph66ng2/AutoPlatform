use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveError {
    NotAuthorized,
    CredentialsAbsent,
}

/// Sandbox live só depois de confirmação humana fora do Git.
pub fn live_sandbox() -> Result<(), LiveError> {
    if env::var("AUTO_PLATFORM_LIVE_SANDBOX").ok().as_deref() != Some("1") {
        return Err(LiveError::NotAuthorized);
    }
    Err(LiveError::CredentialsAbsent)
}
