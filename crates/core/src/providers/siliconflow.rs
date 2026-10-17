//! SiliconFlow. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct SiliconFlow;

const ENDPOINT: &str = "https://api.siliconflow.com/v1/user/info";
const HOST: &str = "api.siliconflow.com";

#[async_trait]
impl Provider for SiliconFlow {
    fn id(&self) -> &'static str {
        "siliconflow"
    }

    fn display_name(&self) -> &'static str {
        "SiliconFlow"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info"
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

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported("This provider is not wired up yet.".into()))
    }
}
