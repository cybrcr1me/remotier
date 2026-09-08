//! The HTTP half. Rust, not the webview - `tauri.conf.json`'s CSP is `connect-src 'self'`
//! and the capability set grants no `http` plugin, both deliberately.

use std::time::Duration;

use remotier_sync_proto::api::{
    InstanceInfo, LoginRequest, PullResponse, PushRequest, PushResponse, RecoverRequest,
    RefreshRequest, RegisterRequest, Session, Share, ShareRequest, UserLookup,
};
use remotier_sync_proto::record::Envelope;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

pub struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    pub fn new(base_url: &str) -> Result<Self> {
        install_crypto_provider();

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("remotier/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Sync(format!("could not build the http client: {e}")))?;

        Ok(Self {
            http,
            base: base_url.trim_end_matches('/').to_string(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Ask an address whether it is a Remotier instance, before anything is sent to it.
    ///
    /// This is what turns a typo into "no Remotier instance there" rather than a failed
    /// login against a stranger's server.
    pub async fn instance(&self) -> Result<InstanceInfo> {
        let response = self
            .http
            .get(self.url("/v1/instance"))
            // Short: this runs while someone is typing a URL, and a hung probe with no
            // answer reads as the app being broken.
            .timeout(Duration::from_secs(8))
            .send()
            .await?;
        json(response).await
    }

    pub async fn register(&self, body: &RegisterRequest) -> Result<Session> {
        let response = self.http.post(self.url("/v1/auth/register")).json(body).send().await?;
        json(response).await
    }

    pub async fn login(&self, body: &LoginRequest) -> Result<Session> {
        let response = self.http.post(self.url("/v1/auth/login")).json(body).send().await?;
        json(response).await
    }

    pub async fn recover(&self, body: &RecoverRequest) -> Result<Session> {
        let response = self.http.post(self.url("/v1/auth/recover")).json(body).send().await?;
        json(response).await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<Session> {
        let response = self
            .http
            .post(self.url("/v1/auth/refresh"))
            .json(&RefreshRequest {
                refresh_token: refresh_token.to_string(),
            })
            .send()
            .await?;
        json(response).await
    }

    pub async fn logout(&self, access: &str) -> Result<()> {
        let response = self
            .http
            .post(self.url("/v1/auth/logout"))
            .bearer_auth(access)
            .send()
            .await?;
        expect_ok(response).await
    }

    pub async fn pull(&self, access: &str, cursor: i64) -> Result<PullResponse> {
        let response = self
            .http
            .get(self.url("/v1/sync"))
            .query(&[("cursor", cursor)])
            .bearer_auth(access)
            .send()
            .await?;
        json(response).await
    }

    pub async fn push(&self, access: &str, envelopes: Vec<Envelope>) -> Result<PushResponse> {
        let response = self
            .http
            .post(self.url("/v1/sync"))
            .bearer_auth(access)
            .json(&PushRequest { envelopes })
            .send()
            .await?;
        json(response).await
    }

    pub async fn lookup(&self, access: &str, email: &str) -> Result<UserLookup> {
        let response = self
            .http
            .get(self.url("/v1/users/lookup"))
            .query(&[("email", email)])
            .bearer_auth(access)
            .send()
            .await?;
        json(response).await
    }

    pub async fn share(&self, access: &str, body: &ShareRequest) -> Result<()> {
        let response = self
            .http
            .post(self.url("/v1/shares"))
            .bearer_auth(access)
            .json(body)
            .send()
            .await?;
        expect_ok(response).await
    }

    /// Every share this account is a member of, with the group key wrapped for it.
    pub async fn shares_for_me(&self, access: &str) -> Result<Vec<Share>> {
        let response = self
            .http
            .get(self.url("/v1/shares"))
            .bearer_auth(access)
            .send()
            .await?;
        json(response).await
    }

    pub async fn shares(&self, access: &str, group_id: &str) -> Result<Vec<Share>> {
        let response = self
            .http
            .get(self.url(&format!("/v1/shares/{group_id}")))
            .bearer_auth(access)
            .send()
            .await?;
        json(response).await
    }

    pub async fn unshare(&self, access: &str, group_id: &str, user_id: &str) -> Result<()> {
        let response = self
            .http
            .delete(self.url(&format!("/v1/shares/{group_id}/{user_id}")))
            .bearer_auth(access)
            .send()
            .await?;
        expect_ok(response).await
    }
}

async fn json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let response = check(response).await?;
    let body = response.text().await?;
    serde_json::from_str(&body).map_err(|e| {
        // The body is the only clue when a proxy or a captive portal answers instead of
        // the server, and "expected value at line 1" on its own says nothing.
        log::debug!("sync response was not the expected shape: {body}");
        Error::Sync(format!("the server sent something unexpected: {e}"))
    })
}

async fn expect_ok(response: reqwest::Response) -> Result<()> {
    check(response).await.map(|_| ())
}

/// Turn a non-2xx into the right error, reading the server's own message when it sent one.
async fn check(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }

    // 401 is its own variant: nothing this device can do will make the token valid, so
    // the UI signs out rather than retrying forever.
    let unauthorised = response.status() == reqwest::StatusCode::UNAUTHORIZED;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if unauthorised {
        return Err(Error::SyncUnauthorised);
    }

    let message = serde_json::from_str::<remotier_sync_proto::api::ApiError>(&body)
        .map(|e| e.message)
        .unwrap_or_else(|_| format!("the server answered {status}"));
    Err(Error::Sync(message))
}

/// rustls 0.23 has no default provider unless a feature picks one, and reqwest is built
/// here without the feature that would pick aws-lc-rs. Installing ring explicitly is what
/// makes TLS work at all; without it every request fails at handshake with an error that
/// does not mention providers.
fn install_crypto_provider() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Fails only if something else installed one first, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
