//! Cache Copilot model metadata separately from credentials so refresh cannot undo logout.

use super::AuthManager;
use super::CodexAuth;
use super::GitHubCopilotAuth;
use crate::GitHubCopilotLoginOptions;
use crate::github_copilot::refresh_github_copilot_auth;
use codex_protocol::openai_models::ReasoningEffort;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const CACHE_FILE: &str = "github_copilot_models.json";

#[derive(Deserialize, Serialize)]
struct ModelCache {
    identity: String,
    models: Vec<String>,
    reasoning_efforts: BTreeMap<String, Vec<ReasoningEffort>>,
}

fn identity(auth: &GitHubCopilotAuth) -> String {
    let mut hash = Sha256::new();
    for value in [
        Some(auth.access_token()),
        Some(auth.api_endpoint()),
        auth.login(),
        auth.copilot_sku(),
    ] {
        match value {
            Some(value) => {
                hash.update([1]);
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            }
            None => hash.update([0]),
        }
    }
    format!("{:x}", hash.finalize())
}

pub(super) async fn load_model_cache(
    home: &Path,
    auth: &GitHubCopilotAuth,
) -> std::io::Result<Option<GitHubCopilotAuth>> {
    let contents = match tokio::fs::read(home.join(CACHE_FILE)).await {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let cache: ModelCache = serde_json::from_slice(&contents)?;
    if cache.identity != identity(auth) {
        return Ok(None);
    }
    GitHubCopilotAuth::new(
        auth.access_token().to_string(),
        auth.api_endpoint().to_string(),
        auth.login().map(str::to_string),
        auth.copilot_sku().map(str::to_string),
        cache.models,
    )
    .map(|auth| Some(auth.with_model_reasoning_efforts(cache.reasoning_efforts)))
}

impl AuthManager {
    /// Refreshes the Copilot catalog without rewriting credentials or blocking cached reads.
    /// Failed refreshes preserve the last catalog; overlapping refreshes are coalesced.
    pub async fn refresh_github_copilot_models(
        self: &Arc<Self>,
        options: &GitHubCopilotLoginOptions,
    ) -> std::io::Result<()> {
        let Ok(_refresh_guard) = self.refresh_lock.try_acquire() else {
            return Ok(());
        };
        let Some(CodexAuth::GitHubCopilot(expected)) = self.auth_cached() else {
            return Ok(());
        };
        let refreshed = tokio::time::timeout(
            Duration::from_secs(/*secs*/ 10),
            refresh_github_copilot_auth(options, &expected),
        )
        .await
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))??;
        let manager = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            manager.persist_github_copilot_models(expected, refreshed)
        })
        .await
        .map_err(std::io::Error::other)?
    }

    fn persist_github_copilot_models(
        &self,
        expected: GitHubCopilotAuth,
        refreshed: GitHubCopilotAuth,
    ) -> std::io::Result<()> {
        if !matches!(self.auth_cached(), Some(CodexAuth::GitHubCopilot(current)) if current == expected)
        {
            return Ok(());
        }
        let cache = ModelCache {
            identity: identity(&refreshed),
            models: refreshed.models().to_vec(),
            reasoning_efforts: refreshed
                .models()
                .iter()
                .map(|model| {
                    (
                        model.clone(),
                        refreshed.reasoning_efforts_for_model(model).to_vec(),
                    )
                })
                .collect(),
        };
        std::fs::create_dir_all(&self.codex_home)?;
        let temporary = self.codex_home.join(format!(
            ".github_copilot_models-{:016x}.tmp",
            rand::random::<u64>()
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let saved = (|| {
            file.write_all(&serde_json::to_vec(&cache)?)?;
            drop(file);
            std::fs::rename(&temporary, self.codex_home.join(CACHE_FILE))
        })();
        if saved.is_err() {
            std::fs::remove_file(&temporary)?;
        }
        // Disk/keyring I/O never runs under the cache lock used by foreground auth reads.
        let mut cached = self
            .inner
            .write()
            .map_err(|_| std::io::Error::other("auth cache lock poisoned"))?;
        if matches!(&cached.auth, Some(CodexAuth::GitHubCopilot(current)) if current == &expected) {
            // Metadata updates do not publish credential/owner revisions or invalidate requests.
            cached.auth = Some(CodexAuth::GitHubCopilot(refreshed));
        }
        saved
    }
}

#[cfg(test)]
#[path = "github_copilot_refresh_tests.rs"]
mod tests;
