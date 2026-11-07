//! HTTP client for the NotiFansly Go API.
//!
//! `ApiClient` owns a small dedicated tokio runtime so blocking calls can be
//! made safely from GPUI background-executor threads (same pattern as the
//! an earlier internal GPUI codebase GPUI client).

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
struct ClaimRequest {
    claim_token: String,
}

#[derive(Debug, Clone, Serialize)]
struct SyncKeyRequest {
    sync_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectResponse {
    #[serde(default, rename = "fanslyUserId")]
    pub fansly_user_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub sync_key: String,
    #[serde(default)]
    pub created: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StatusResponse {
    #[serde(default, rename = "fanslyUserId")]
    pub fansly_user_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default, rename = "syncMethod")]
    pub sync_method: String,
    #[serde(default, rename = "tokenStatus")]
    pub token_status: String,
    #[serde(default, rename = "lastSyncAt")]
    pub last_sync_at: Option<String>,
    #[serde(default, rename = "discordLinked")]
    pub discord_linked: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiError {
    #[serde(default)]
    error: String,
}

/// The latest GitHub release of the desktop app itself.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LatestRelease {
    #[serde(default, rename = "tag_name")]
    pub tag_name: String,
    #[serde(default, rename = "html_url")]
    pub html_url: String,
}

pub const RELEASES_URL: &str = "https://github.com/NotiFansly/NotiFansly-Desktop/releases/latest";

/// Compare two dotted versions ("v0.2.0" vs "0.1.0"); true when `remote`
/// is newer than `local`. Unparseable segments count as 0.
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let r = parse(remote);
    let l = parse(local);
    for i in 0..r.len().max(l.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv != lv {
            return rv > lv;
        }
    }
    false
}

/// Accept the full management link or the bare claim token (server-side also
/// tolerates both, but stripping here keeps the payload small and the error
/// messages local).
pub fn extract_claim_token(input: &str) -> String {
    let mut token = input.trim();
    if let Some(i) = token.find("/claim/") {
        token = &token[i + "/claim/".len()..];
        if let Some(j) = token.find(['?', '#']) {
            token = &token[..j];
        }
    }
    token.trim_end_matches('/').to_string()
}

pub struct ApiClient {
    inner: reqwest::Client,
    rt: Arc<tokio::runtime::Runtime>,
    api_url: String,
}

impl ApiClient {
    pub fn new(api_url: &str) -> Self {
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("failed to build tokio runtime"),
        );
        let inner = reqwest::Client::builder()
            .user_agent("NotiFansly-Desktop/0.1")
            .timeout(Duration::from_secs(45))
            .build()
            .expect("failed to build http client");
        Self {
            inner,
            rt,
            api_url: api_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        self.rt.block_on(fut)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_url, path)
    }

    async fn post_json(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<reqwest::Response, String> {
        self.inner
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .map_err(|e| match e {
                e if e.is_timeout() => format!("The server took too long to respond. Error: {e}"),
                e if e.is_connect() => format!("Could not reach the sync server. Check your connection and the API server address. ({e})"),
                e => format!("Network request failed: {e}"),
            })
    }

    /// Human-readable error body from an API error response.
    async fn error_body(status: reqwest::StatusCode, resp: reqwest::Response) -> String {
        match resp.json::<ApiError>().await {
            Ok(parsed) if !parsed.error.is_empty() => parsed.error,
            _ => format!("The server returned an error (HTTP {status})."),
        }
    }

    /// Claim a management session link. For an existing creator the same sync
    /// key is returned again, so this is also the recovery flow.
    pub fn connect(&self, claim_token: &str) -> Result<ConnectResponse, String> {
        let token = extract_claim_token(claim_token);
        self.block_on(async move {
            if token.len() < 16 {
                return Err("That link looks too short - paste the full management session link from your Fansly creator dashboard.".to_string());
            }
            let resp = self.post_json("/api/v1/desktop/connect", &ClaimRequest { claim_token: token })
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(Self::error_body(status, resp).await);
            }
            resp.json::<ConnectResponse>()
                .await
                .map_err(|e| format!("Unexpected server response: {e}"))
        })
    }

    /// Current server-side sync state for the creator owning this sync key.
    pub fn status(&self, sync_key: &str) -> Result<StatusResponse, String> {
        self.block_on(async move {
            let resp = self.post_json("/api/v1/desktop/status", &SyncKeyRequest { sync_key: sync_key.to_string() })
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(Self::error_body(status, resp).await);
            }
            resp.json::<StatusResponse>()
                .await
                .map_err(|e| format!("Unexpected server response: {e}"))
        })
    }

    /// The latest published release of this app on GitHub.
    pub fn latest_release(&self) -> Result<LatestRelease, String> {
        self.block_on(async move {
            self.inner
                .get("https://api.github.com/repos/NotiFansly/NotiFansly-Desktop/releases/latest")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
                .map_err(|e| format!("release check failed: {e}"))?
                .json::<LatestRelease>()
                .await
                .map_err(|e| format!("unexpected release response: {e}"))
        })
    }

    /// Kick off a server-side sync. Returns once the request is accepted;
    /// the caller polls [Self::status] to watch `last_sync_at` move.
    pub fn trigger_sync(&self, sync_key: &str) -> Result<(), String> {
        self.block_on(async move {
            let resp = self.post_json("/api/v1/desktop/trigger-sync", &SyncKeyRequest { sync_key: sync_key.to_string() })
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(Self::error_body(status, resp).await);
            }
            Ok(())
        })
    }
}
