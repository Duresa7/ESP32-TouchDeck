//! Integration descriptors and runtime contract. Modules register their factory once.
mod cliproxy;
pub mod secrets;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use ts_rs::TS;

#[derive(Clone, Serialize, Deserialize, TS)]
pub struct ConfigField {
    pub name: String,
    pub kind: String,
    pub secret: bool,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct DataStream {
    pub id: String,
    pub kind: String,
    pub unit: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct PresetTile {
    pub id: String,
    pub name: String,
    pub action: Value,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct Delivery {
    pub device: String,
    pub sent: u64,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct IntegrationStatus {
    pub state: String,
    pub message: Option<String>,
    pub delivery: Vec<Delivery>,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct IntegrationSummary {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub enabled: bool,
    pub status: IntegrationStatus,
    pub config_schema: Vec<ConfigField>,
    pub streams: Vec<DataStream>,
    pub actions: Vec<String>,
    pub presets: Vec<PresetTile>,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct SecretState {
    pub set: bool,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct IntegrationView {
    pub summary: IntegrationSummary,
    pub config: Value,
    pub secrets: BTreeMap<String, SecretState>,
}
#[derive(Debug)]
pub enum Error {
    NotFound,
    ConfigInvalid,
    Credentials,
    Storage,
}

pub trait Integration: Send {
    fn summary(&self) -> IntegrationSummary;
    fn view(&self) -> Result<IntegrationView, Error>;
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error>;
    fn secret_put(&mut self, field: &str, value: &str) -> Result<(), Error>;
    fn secret_delete(&mut self, field: &str) -> Result<(), Error>;
    fn refresh(&mut self);
    fn tick(&mut self);
    /// Latest Live tile values, consumed by the Device scheduler, not the HTTP API.
    fn readings(&self) -> Vec<crate::sources::Reading>;
    /// The Device service reports delivery independently of upstream polling.
    fn delivered(&mut self, delivery: Delivery);
}
pub struct Environment {
    pub secrets: Arc<dyn secrets::SecretStore>,
    pub config_file: Option<PathBuf>,
    pub fetch: Arc<dyn Fn() -> Box<dyn crate::sources::Fetch> + Send + Sync>,
}
pub struct Registration(pub fn(&Environment) -> Result<Box<dyn Integration>, Error>);
inventory::collect!(Registration);
pub struct Registry(BTreeMap<String, Box<dyn Integration>>);
impl Registry {
    pub fn new(env: Environment) -> Result<Self, Error> {
        Self::from_registrations(env, inventory::iter::<Registration>)
    }
    fn from_registrations<'a>(env: Environment, registrations: impl IntoIterator<Item = &'a Registration>) -> Result<Self, Error> {
        let mut entries = BTreeMap::new();
        for r in registrations {
            let i = (r.0)(&env)?;
            let summary = i.summary();
            secrets::validate_id(&summary.id).map_err(|_| Error::ConfigInvalid)?;
            for field in &summary.config_schema {
                secrets::validate_id(&field.name).map_err(|_| Error::ConfigInvalid)?;
            }
            if entries.insert(summary.id, i).is_some() {
                return Err(Error::ConfigInvalid);
            }
        }
        Ok(Self(entries))
    }
    pub fn list(&self) -> Vec<IntegrationSummary> {
        self.0.values().map(|i| i.summary()).collect()
    }
    pub fn get(&mut self, id: &str) -> Result<&mut Box<dyn Integration>, Error> {
        self.0.get_mut(id).ok_or(Error::NotFound)
    }
    pub fn tick(&mut self) {
        for i in self.0.values_mut() {
            i.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_registration_is_rejected() {
        let env = Environment {
            secrets: Arc::new(secrets::MemorySecrets::default()),
            config_file: None,
            fetch: Arc::new(|| Box::new(crate::sources::HttpFetch::new())),
        };
        let registration = inventory::iter::<Registration>.into_iter().next().unwrap();
        assert!(Registry::from_registrations(env, [registration, registration]).is_err());
    }
}
