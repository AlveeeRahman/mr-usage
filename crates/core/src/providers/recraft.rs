//! Recraft. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Recraft;

const ENDPOINT: &str = "https://external.api.recraft.ai/v1/users/me";
const HOST: &str = "external.api.recraft.ai";

#[async_trait]
impl Provider for Recraft {
    fn id(&self) -> &'static str {
        "recraft"
    }

    fn display_name(&self) -> &'static str {
        "Recraft"
    }

    fn docs_url(&self) -> &'static str {
        "https://www.recraft.ai/docs/api-reference/endpoints"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Credits
    }

    fn category(&self) -> Category {
        Category::Media
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
