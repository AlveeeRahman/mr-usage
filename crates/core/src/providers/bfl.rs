//! Black Forest Labs. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct BlackForestLabs;

const ENDPOINT: &str = "https://api.bfl.ai/v1/credits";
const HOST: &str = "api.bfl.ai";

#[async_trait]
impl Provider for BlackForestLabs {
    fn id(&self) -> &'static str {
        "bfl"
    }

    fn display_name(&self) -> &'static str {
        "Black Forest Labs"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.bfl.ml/api-reference/get-the-users-credits"
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
