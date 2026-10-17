//! IBM Quantum. Dropped: the app cannot authenticate to it.
//!
//! Checked 2026-10-17 against
//! <https://quantum.cloud.ibm.com/docs/en/api/qiskit-runtime-rest>
//! ("IBM Quantum Compute Service REST API", Authentication): "You must
//! provide an IBM Cloud Identity and Access Management (IAM) bearer token with
//! every call as an http header. You can generate this using your API key".
//! The token comes from `POST https://iam.cloud.ibm.com/identity/token` with
//! `grant_type=urn:ibm:params:oauth:grant-type:apikey&apikey=…`, expires
//! within an hour, and is then sent as `Authorization: Bearer <token>` with
//! `Service-CRN: <instance CRN>` and `IBM-API-Version: <YYYY-MM-DD>` to
//! `https://quantum.cloud.ibm.com/api/v1/…`. The page documents no way to
//! send the API key itself to the quantum host.
//!
//! A provider may contact exactly one host, and `iam.cloud.ibm.com` is not
//! `quantum.cloud.ibm.com`, so the usage call (`/api/v1/instances/usage`,
//! which reports consumed and limit seconds for the instance) cannot be made.
//! `fetch` reports this as `Unsupported`. The endpoint stays a template
//! because the provider keeps its "Instance CRN" setting for the day the
//! exchange can be done.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::net::Http;
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct IbmQuantum;

const ENDPOINT: &str = "https://quantum.cloud.ibm.com/api/v1/instances/usage (Service-CRN: <instance CRN>)";
const HOST: &str = "quantum.cloud.ibm.com";

pub(crate) const UNSUPPORTED: &str = "IBM Quantum only accepts an IAM bearer token, which has to be minted from the API key at iam.cloud.ibm.com. \
This app contacts one host per provider and does not call IBM's token service, so it cannot read this usage.";

#[async_trait]
impl Provider for IbmQuantum {
    fn id(&self) -> &'static str {
        "ibmquantum"
    }

    fn display_name(&self) -> &'static str {
        "IBM Quantum"
    }

    fn docs_url(&self) -> &'static str {
        "https://quantum.cloud.ibm.com/docs/en/api/qiskit-runtime-rest"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Seconds
    }

    fn category(&self) -> Category {
        Category::Quantum
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Not available: IBM requires an IAM token exchange on a second host, which this app does not do.")
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Instance CRN",
            hint: "The service instance CRN from IBM Cloud. Not a key.",
            placeholder: "crn:v1:bluemix:public:quantum-computing:us-east:…",
        })
    }

    async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
        Err(ProviderError::Unsupported(UNSUPPORTED.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_reports_unsupported_without_touching_the_network() {
        let http = Http::new().unwrap();
        let key = SecretString::from("not-a-real-key".to_owned());
        let result = IbmQuantum
            .fetch(&key, Some("crn:v1:bluemix:public:quantum-computing:us-east:a/x::"), &http)
            .await;
        assert_eq!(result, Err(ProviderError::Unsupported(UNSUPPORTED.into())));
        assert!(UNSUPPORTED.contains("iam.cloud.ibm.com"));
    }

    #[test]
    fn endpoint_is_a_template_because_a_setting_is_required() {
        assert!(IbmQuantum.config_field().is_some());
        assert!(IbmQuantum.endpoint().contains('<'));
    }
}
