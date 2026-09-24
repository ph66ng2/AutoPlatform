//! Núcleo de estoque. Saldo não é coluna editável e reserva não altera on_hand.
//!
//! Mutação offline é recusada. Este crate não referencia outros domínios.

mod api;
mod model;

pub use api::{
    availability, commit_reservation, consume, expire_reservation, receive, reconcile,
    register_product, release_reservation, reserve,
};
pub use model::{Availability, CommandOutcome, InventoryError};

ap_kernel::declare_module!("inventory");

#[cfg(test)]
mod tests {
    use super::{Availability, InventoryError};

    #[test]
    fn declares_its_boundary() {
        assert_eq!(super::boundary().name, super::MODULE_NAME);
        assert!(ap_kernel::REQUIRED_MODULES.contains(&super::MODULE_NAME));
    }

    #[test]
    fn available_is_on_hand_minus_reserved() {
        let stock = Availability::new(5, 2).expect("saldo");
        assert_eq!(stock.available, 3);
        assert_eq!(Availability::new(1, 2), Err(InventoryError::Invariant));
        assert_eq!(Availability::new(-1, 0), Err(InventoryError::Invariant));
    }
}
