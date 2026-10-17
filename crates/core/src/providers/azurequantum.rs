//! Azure Quantum. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct AzureQuantum;

const ENDPOINT: &str = "https://<workspace address>/quotas?api-version=2026-01-15-preview";
const HOST: &str = "*.quantum.azure.com";

#[async_trait]
impl Provider for AzureQuantum {
    fn id(&self) -> &'static str {
        "azurequantum"
    }

    fn display_name(&self) -> &'static str {
        "Azure Quantum"
    }

    fn docs_url(&self) -> &'static str {
        "https://learn.microsoft.com/en-us/rest/api/azurequantum/dataplane/quotas/list?view=rest-azurequantum-dataplane-2026-01-15-preview"
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
            label: "Workspace address",
            hint: "The workspace's data-plane address and resource path from the Azure portal, starting with https://. Not a key.",
            placeholder: "https://myws.eastus.quantum.azure.com/subscriptions/…/workspaces/myws",
        })
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
