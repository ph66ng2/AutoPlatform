use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct IdempotencyStore {
    seen: BTreeMap<String, u64>,
}

impl IdempotencyStore {
    pub fn apply(&mut self, key: &str, amount_cents: u64) -> u64 {
        *self.seen.entry(key.to_string()).or_insert(amount_cents)
    }

    #[must_use]
    pub fn total(&self) -> u64 {
        self.seen.values().copied().sum()
    }
}
