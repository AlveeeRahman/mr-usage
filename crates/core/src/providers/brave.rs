//! Brave Search. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Brave;

const ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const HOST: &str = "api.search.brave.com";

#[async_trait]
impl Provider for Brave {
    fn id(&self) -> &'static str {
        "brave"
    }

    fn display_name(&self) -> &'static str {
        "Brave Search"
    }

    fn docs_url(&self) -> &'static str {
        "https://api-dashboard.search.brave.com/documentation/guides/rate-limiting"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Searches
    }

    fn category(&self) -> Category {
        Category::Search
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
