//! RunPod. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct RunPod;

const ENDPOINT: &str = "https://api.runpod.io/graphql";
const HOST: &str = "api.runpod.io";

#[async_trait]
impl Provider for RunPod {
    fn id(&self) -> &'static str {
        "runpod"
    }

    fn display_name(&self) -> &'static str {
        "RunPod"
    }

    fn docs_url(&self) -> &'static str {
        "https://graphql-spec.runpod.io/"
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
        Category::Compute
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
