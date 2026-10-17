//! xAI. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct XAi;

const ENDPOINT: &str = "https://management-api.x.ai/v1/billing/teams/<team id>/prepaid/balance";
const HOST: &str = "management-api.x.ai";

#[async_trait]
impl Provider for XAi {
    fn id(&self) -> &'static str {
        "xai"
    }

    fn display_name(&self) -> &'static str {
        "xAI"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.x.ai/developers/rest-api-reference/management/billing"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Usd
    }

    fn category(&self) -> Category {
        Category::Llm
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Team id",
            hint: "The team id from the xAI console. Not a key.",
            placeholder: "team_…",
        })
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
