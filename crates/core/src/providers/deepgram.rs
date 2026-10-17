//! Deepgram. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct Deepgram;

const ENDPOINT: &str = "https://api.deepgram.com/v1/projects/<project id>/balances";
const HOST: &str = "api.deepgram.com";

#[async_trait]
impl Provider for Deepgram {
    fn id(&self) -> &'static str {
        "deepgram"
    }

    fn display_name(&self) -> &'static str {
        "Deepgram"
    }

    fn docs_url(&self) -> &'static str {
        "https://developers.deepgram.com/reference/manage/billing/list"
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
        Category::Media
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Project id",
            hint: "The project id from the Deepgram console. Not a key.",
            placeholder: "4a8e…",
        })
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
