//! Persistent local state (config-dir JSON).
//!
//! Stores the API base URL and, once connected, the creator's identity +
//! sync key so restarts don't need a fresh management link. The file is
//! written world-inaccessible on Unix (0600) since it holds a credential.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub const DEFAULT_API_URL: &str = "https://api.notifansly.xyz";
pub const DEFAULT_DASHBOARD_URL: &str = "https://notifansly.xyz";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub dashboard_url: Option<String>,
    #[serde(default)]
    pub fansly_user_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub sync_key: Option<String>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            api_url: None,
            dashboard_url: None,
            fansly_user_id: None,
            username: None,
            sync_key: None,
        }
    }
}

impl PersistedState {
    pub fn api_url(&self) -> &str {
        self.api_url.as_deref().unwrap_or(DEFAULT_API_URL)
    }

    pub fn dashboard_url(&self) -> &str {
        self.dashboard_url.as_deref().unwrap_or(DEFAULT_DASHBOARD_URL)
    }

    pub fn is_connected(&self) -> bool {
        self.sync_key.as_deref().map(|k| !k.is_empty()).unwrap_or(false)
    }
}

pub fn state_path() -> PathBuf {
    let mut p = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notifansly-desktop");
    let _ = fs::create_dir_all(&p);
    p.push("state.json");
    p
}

pub fn load() -> PersistedState {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save via temp-then-rename so a crash mid-write can't corrupt state, with
/// 0600 permissions from creation (the file holds the sync key credential).
pub fn save(state: &PersistedState) {
    let path = state_path();
    let Ok(json) = serde_json::to_string_pretty(state) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    let write_result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&tmp, &path)
    })();
    if let Err(e) = write_result {
        eprintln!("[state] failed to save state: {e:?}");
    }
}
