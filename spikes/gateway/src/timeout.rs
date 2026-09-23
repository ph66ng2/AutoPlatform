#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    Success,
    Failure,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderState {
    Succeeded,
    Failed,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MustQuery;

#[must_use]
pub fn after_attempt(attempt: Attempt) -> ProviderState {
    match attempt {
        Attempt::Success => ProviderState::Succeeded,
        Attempt::Failure => ProviderState::Failed,
        Attempt::Timeout => ProviderState::Indeterminate,
    }
}

pub fn retry(state: ProviderState) -> Result<(), MustQuery> {
    match state {
        ProviderState::Indeterminate => Err(MustQuery),
        ProviderState::Succeeded | ProviderState::Failed => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Charged,
    Refunded,
    ChargedBack,
}
