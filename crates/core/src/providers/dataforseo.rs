//! DataForSEO. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct DataForSeo;

const ENDPOINT: &str = "https://api.dataforseo.com/v3/appendix/user_data";
const HOST: &str = "api.dataforseo.com";

#[async_trait]
impl Provider for DataForSeo {
    fn id(&self) -> &'static str {
        "dataforseo"
    }

    fn display_name(&self) -> &'static str {
        "DataForSEO"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.dataforseo.com/v3/appendix/user_data/"
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
        Category::Seo
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
