//! Manually registered assets. These are separate from EasyTier registered machines.
pub(crate) mod address_derivation;
pub(crate) mod binding_ops;
mod bindings;
pub(crate) mod common;
pub(crate) mod devices;
mod groups;
mod idempotency;
mod instances;
pub(crate) mod publications;
mod services;
mod types;
pub(crate) mod validation;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod binding_helper_tests;

pub(crate) use common::AssetError;
pub(crate) use devices::DeviceListQuery;
pub(crate) use instances::{RuntimeObservation, RuntimeObservations};
pub(crate) use publications::PublicationRuntimeProof;

use super::Db;
use sqlx::SqlitePool;

#[derive(Clone)]
pub(crate) struct AssetsStore {
    pool: SqlitePool,
    runtime_observations: RuntimeObservations,
    publication_runtime_proof: Option<PublicationRuntimeProof>,
}

impl AssetsStore {
    pub(crate) fn new(db: Db) -> Self {
        Self {
            pool: db.inner(),
            runtime_observations: RuntimeObservations::default(),
            publication_runtime_proof: None,
        }
    }

    pub(crate) fn with_runtime_observations(mut self, observations: RuntimeObservations) -> Self {
        self.runtime_observations = observations;
        self
    }

    pub(crate) fn with_publication_runtime_proof(mut self, proof: Option<PublicationRuntimeProof>) -> Self {
        self.publication_runtime_proof = proof;
        self
    }
}
