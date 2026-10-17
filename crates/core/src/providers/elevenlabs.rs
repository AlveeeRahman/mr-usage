//! ElevenLabs. Stub: the parser is not written yet, so `fetch` reports the
//! provider as unsupported. The endpoint and documentation page come from
//! the 1.0.0 survey and are confirmed when the parser lands.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct ElevenLabs;

const ENDPOINT: &str = "https://api.elevenlabs.io/v1/user/subscription";
const HOST: &str = "api.elevenlabs.io";

#[async_trait]
impl Provider for ElevenLabs {
    fn id(&self) -> &'static str {
        "elevenlabs"
    }

    fn display_name(&self) -> &'static str {
        "ElevenLabs"
    }

    fn docs_url(&self) -> &'static str {
        "https://elevenlabs.io/docs/api-reference/user/subscription/get"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Characters
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
