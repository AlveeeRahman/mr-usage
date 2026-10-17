//! Majestic. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Majestic;

const ENDPOINT: &str = "https://api.majestic.com/api/json?cmd=GetSubscriptionInfo";
const HOST: &str = "api.majestic.com";

#[async_trait]
impl Provider for Majestic {
    fn id(&self) -> &'static str {
        "majestic"
    }

    fn display_name(&self) -> &'static str {
        "Majestic"
    }

    fn docs_url(&self) -> &'static str {
        "https://developer-support.majestic.com/api/commands/get-subscription-info.shtml"
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
        Category::Seo
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
