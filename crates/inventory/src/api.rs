use postgres::Client;
use serde::Deserialize;
use uuid::Uuid;

use crate::model::{Availability, CommandOutcome, InventoryError};

pub fn register_product(client: &mut Client, sku: &str) -> Result<Uuid, InventoryError> {
    let row = client
        .query_one("SELECT inventory.register_product($1)", &[&sku])
        .map_err(map_db)?;
    Ok(row.get(0))
}

pub fn availability(client: &mut Client, sku: &str) -> Result<Availability, InventoryError> {
    let raw = scalar_json(client, "SELECT inventory.availability($1)::text", &[&sku])?;
    let stock: Availability = serde_json::from_str(&raw).map_err(|_| InventoryError::Protocol)?;
    Availability::new(stock.on_hand, stock.reserved)?;
    if stock.available != stock.on_hand - stock.reserved {
        return Err(InventoryError::Invariant);
    }
    Ok(stock)
}

pub fn receive(
    client: &mut Client,
    sku: &str,
    quantity: i32,
    operation_id: &str,
) -> Result<CommandOutcome, InventoryError> {
    command(
        client,
        "SELECT inventory.receive($1, $2, $3)::text",
        &[&sku, &quantity, &operation_id],
    )
}

pub fn consume(
    client: &mut Client,
    sku: &str,
    quantity: i32,
    operation_id: &str,
) -> Result<CommandOutcome, InventoryError> {
    command(
        client,
        "SELECT inventory.consume($1, $2, $3)::text",
        &[&sku, &quantity, &operation_id],
    )
}

pub fn reserve(
    client: &mut Client,
    sku: &str,
    quantity: i32,
    operation_id: &str,
    ttl: &str,
) -> Result<CommandOutcome, InventoryError> {
    command(
        client,
        "SELECT inventory.reserve($1, $2, $3, $4)::text",
        &[&sku, &quantity, &operation_id, &ttl],
    )
}

pub fn commit_reservation(
    client: &mut Client,
    reserve_operation_id: &str,
    operation_id: &str,
) -> Result<CommandOutcome, InventoryError> {
    command(
        client,
        "SELECT inventory.commit_reservation($1, $2)::text",
        &[&reserve_operation_id, &operation_id],
    )
}

pub fn release_reservation(
    client: &mut Client,
    reserve_operation_id: &str,
    operation_id: &str,
) -> Result<CommandOutcome, InventoryError> {
    command(
        client,
        "SELECT inventory.release_reservation($1, $2)::text",
        &[&reserve_operation_id, &operation_id],
    )
}

pub fn expire_reservation(
    client: &mut Client,
    reserve_operation_id: &str,
    operation_id: &str,
) -> Result<CommandOutcome, InventoryError> {
    command(
        client,
        "SELECT inventory.expire_reservation($1, $2)::text",
        &[&reserve_operation_id, &operation_id],
    )
}

pub fn reconcile(client: &mut Client) -> Result<i32, InventoryError> {
    let raw = scalar_json(client, "SELECT inventory.reconcile()::text", &[])?;
    #[derive(Deserialize)]
    struct Expired {
        expired: i32,
    }
    let body: Expired = serde_json::from_str(&raw).map_err(|_| InventoryError::Protocol)?;
    Ok(body.expired)
}

fn command(
    client: &mut Client,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<CommandOutcome, InventoryError> {
    let raw = scalar_json(client, sql, params)?;
    let outcome: CommandOutcome =
        serde_json::from_str(&raw).map_err(|_| InventoryError::Protocol)?;
    Availability::new(outcome.on_hand, outcome.reserved)?;
    if outcome.available != outcome.on_hand - outcome.reserved {
        return Err(InventoryError::Invariant);
    }
    Ok(outcome)
}

fn scalar_json(
    client: &mut Client,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<String, InventoryError> {
    let row = client.query_one(sql, params).map_err(map_db)?;
    Ok(row.get(0))
}

fn map_db(error: postgres::Error) -> InventoryError {
    let message = error
        .as_db_error()
        .map(|db| db.message().to_string())
        .unwrap_or_else(|| error.to_string());
    if message.contains("inventory:denied") {
        InventoryError::Denied
    } else if message.contains("inventory:offline") {
        InventoryError::Offline
    } else if message.contains("inventory:insufficient") {
        InventoryError::Insufficient
    } else if message.contains("inventory:sku") || message.contains("inventory:reservation") {
        InventoryError::NotFound
    } else if message.contains("inventory:kind") {
        InventoryError::Conflict
    } else if message.contains("inventory:invariant") {
        InventoryError::Invariant
    } else {
        InventoryError::Protocol
    }
}
