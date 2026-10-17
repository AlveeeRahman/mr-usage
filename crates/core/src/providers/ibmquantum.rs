//! IBM Quantum. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct IbmQuantum;

const ENDPOINT: &str = "https://quantum.cloud.ibm.com/api/v1/instances/<instance CRN>/usage";
const HOST: &str = "quantum.cloud.ibm.com";

#[async_trait]
impl Provider for IbmQuantum {
    fn id(&self) -> &'static str {
        "ibmquantum"
    }

    fn display_name(&self) -> &'static str {
        "IBM Quantum"
    }

    fn docs_url(&self) -> &'static str {
        "https://quantum.cloud.ibm.com/docs/en/api/qiskit-runtime-rest"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Seconds
    }

    fn category(&self) -> Category {
        Category::Quantum
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Instance CRN",
            hint: "The service instance CRN from IBM Cloud. Not a key.",
            placeholder: "crn:v1:bluemix:public:quantum-computing:us-east:…",
        })
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
