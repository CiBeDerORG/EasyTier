//! Manually registered assets. These are separate from EasyTier registered machines.
mod address_derivation;
mod binding_ops;
mod bindings;
mod common;
mod devices;
mod groups;
mod idempotency;
mod instances;
mod services;
mod types;
mod validation;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod binding_helper_tests;

pub(crate) use common::AssetError;
pub(crate) use devices::DeviceListQuery;
pub(crate) use instances::{RuntimeObservation, RuntimeObservations};

use super::Db;
use sqlx::SqlitePool;

#[derive(Clone)]
pub(crate) struct AssetsStore {
    pool: SqlitePool,
    runtime_observations: RuntimeObservations,
}

impl AssetsStore {
    pub(crate) fn new(db: Db) -> Self {
        Self {
            pool: db.inner(),
            runtime_observations: RuntimeObservations::default(),
        }
    }

    pub(crate) fn with_runtime_observations(mut self, observations: RuntimeObservations) -> Self {
        self.runtime_observations = observations;
        self
    }
}
