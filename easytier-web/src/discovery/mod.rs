pub(crate) mod luci_client;
pub(crate) mod reconnect;

use crate::{db::Db, deploy_agent};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, broadcast};

pub(crate) struct Runtime {
    pub(crate) runner: Option<crate::restful::automation::runner::Runner>,
    pub(crate) credentials: deploy_agent::Service,
    pub(crate) events: broadcast::Sender<Value>,
    locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
}
impl Runtime {
    pub(crate) async fn new(db: Db) -> anyhow::Result<Self> {
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            runner: crate::restful::automation::runner::Runner::from_environment(),
            credentials: deploy_agent::Service::new(db).await?,
            events,
            locks: Mutex::new(HashMap::new()),
        })
    }
    pub(crate) async fn lock(&self, key: String) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|p| p.into_inner());
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(&key).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(AsyncMutex::new(()));
                    locks.insert(key, Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }
}
