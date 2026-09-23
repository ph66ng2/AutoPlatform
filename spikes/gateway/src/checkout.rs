#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmError {
    RedirectDoesNotConfirm,
}

/// Redirect do checkout nunca confirma pagamento. Só webhook + consulta.
pub fn on_checkout_return(_query: &str) -> Result<NeverConfirmed, ConfirmError> {
    Err(ConfirmError::RedirectDoesNotConfirm)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeverConfirmed {}
