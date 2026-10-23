//! Manually registered assets. These are separate from EasyTier registered machines.
mod common;
mod devices;
mod groups;
mod idempotency;
mod services;
mod types;
mod validation;
#[cfg(test)]
mod tests;

pub(crate) use common::AssetError;
pub(crate) use devices::DeviceListQuery;

use super::Db;
use sqlx::SqlitePool;

#[derive(Clone)]
pub(crate) struct AssetsStore {
    pool: SqlitePool,
}

impl AssetsStore {
    pub(crate) fn new(db: Db) -> Self {
        Self { pool: db.inner() }
    }
}
