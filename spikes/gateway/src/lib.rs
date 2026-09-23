//! Spikes de billing e pagamento. Não é o adapter final e não cobra ninguém.
//!
//! Billing da plataforma e pagamento da oficina não compartilham estado.

mod checkout;
mod hmac;
mod idempotency;
mod live;
mod oauth;
mod score;
mod secret;
mod timeout;
mod webhook;

pub use checkout::{on_checkout_return, ConfirmError};
pub use hmac::{constant_eq, hex_lower, hmac_sha256};
pub use idempotency::IdempotencyStore;
pub use live::{live_sandbox, LiveError};
pub use oauth::{DesktopView, WorkshopOAuth};
pub use score::{evaluate, load_matrix, Context, Matrix, Provider, Scorecard, PASS_SCORE};
pub use secret::Secret;
pub use timeout::{after_attempt, retry, Attempt, MustQuery, Outcome, ProviderState};
pub use webhook::{
    verify_asaas, verify_mercado_pago, WebhookError, ASAAS_TOKEN_HEADER, MP_SIGNATURE_HEADER,
};

ap_kernel::declare_module!("gateway-spike");
