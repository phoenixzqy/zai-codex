use super::*;
use crate::AuthCredentialsStoreMode;
use crate::AuthKeyringBackendKind;
use crate::login_with_github_copilot;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::tempdir;

fn auth(models: &[&str]) -> GitHubCopilotAuth {
    serde_json::from_value(json!({
        "access_token": "github-test-token",
        "api_endpoint": "https://api.individual.githubcopilot.com",
        "login": "octocat",
        "copilot_sku": "copilot_individual",
        "models": models,
        "model_reasoning_efforts": {"gpt-6.1-terra": ["low", "high"]}
    }))
    .unwrap()
}

fn stored_auth(home: &Path) -> anyhow::Result<GitHubCopilotAuth> {
    Ok(crate::load_auth_dot_json(
        home,
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?
    .unwrap()
    .github_copilot
    .unwrap())
}

#[tokio::test]
async fn refresh_persists_metadata_without_touching_credentials_or_auth_revisions()
-> anyhow::Result<()> {
    let home = tempdir()?;
    let old = auth(&["gpt-6.1-sol"]);
    login_with_github_copilot(
        home.path(),
        old.clone(),
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;
    let old = stored_auth(home.path())?;
    let refreshed = auth(&["gpt-6.1-sol", "gpt-6.1-terra"])
        .with_model_cache_generation(old.model_cache_generation().map(str::to_string));
    let before = std::fs::read(home.path().join("auth.json"))?;
    let manager = AuthManager::from_auth_for_testing_with_home(
        CodexAuth::GitHubCopilot(old.clone()),
        home.path().to_path_buf(),
    );
    let revisions = manager.auth_change_state_receiver();
    manager.persist_github_copilot_models(old.clone(), refreshed.clone())?;
    assert_eq!(
        manager.auth_cached(),
        Some(CodexAuth::GitHubCopilot(refreshed.clone()))
    );
    assert_eq!(std::fs::read(home.path().join("auth.json"))?, before);
    assert!(!std::fs::read_to_string(home.path().join(CACHE_FILE))?.contains("github-test-token"));
    assert!(!revisions.has_changed()?);
    manager.reload().await;
    assert_eq!(
        manager.auth_cached(),
        Some(CodexAuth::GitHubCopilot(refreshed.clone()))
    );
    assert!(!revisions.has_changed()?);
    for (field, replacement) in [
        ("access_token", "other-token"),
        ("api_endpoint", "https://api.business.githubcopilot.com"),
        ("login", "other-user"),
        ("copilot_sku", "other-sku"),
    ] {
        let mut changed = serde_json::to_value(&old)?;
        changed[field] = json!(replacement);
        assert_eq!(
            load_model_cache(home.path(), &serde_json::from_value(changed)?).await?,
            None
        );
    }
    std::fs::write(home.path().join(CACHE_FILE), "invalid json")?;
    manager.reload().await;
    assert_eq!(manager.auth_cached(), Some(CodexAuth::GitHubCopilot(old)));
    assert_eq!(std::fs::read_dir(home.path())?.count(), 2);
    Ok(())
}

#[tokio::test]
async fn newer_login_supersedes_cached_and_in_flight_metadata_for_the_same_credential()
-> anyhow::Result<()> {
    for models in [["new-login-model"], ["original-model"]] {
        let home = tempdir()?;
        login_with_github_copilot(
            home.path(),
            auth(&["original-model"]),
            AuthCredentialsStoreMode::File,
            AuthKeyringBackendKind::default(),
        )?;
        let original = stored_auth(home.path())?;
        let manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::GitHubCopilot(original.clone()),
            home.path().to_path_buf(),
        );
        manager.persist_github_copilot_models(original, auth(&["cached-model"]))?;
        let Some(CodexAuth::GitHubCopilot(cached)) = manager.auth_cached() else {
            unreachable!()
        };
        let stale_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::GitHubCopilot(cached.clone()),
            home.path().to_path_buf(),
        );
        login_with_github_copilot(
            home.path(),
            auth(&models),
            AuthCredentialsStoreMode::File,
            AuthKeyringBackendKind::default(),
        )?;
        let newer = stored_auth(home.path())?;
        let credentials = std::fs::read(home.path().join("auth.json"))?;
        assert!(cached.same_identity(&newer));
        manager.reload().await;
        assert_eq!(
            manager.auth_cached(),
            Some(CodexAuth::GitHubCopilot(newer.clone()))
        );

        // A refresh begun before login can finish afterward, but its cache stays ineligible.
        stale_manager.persist_github_copilot_models(cached, auth(&["late-stale-model"]))?;
        manager.reload().await;
        assert_eq!(
            manager.auth_cached(),
            Some(CodexAuth::GitHubCopilot(newer.clone()))
        );
        assert_eq!(std::fs::read(home.path().join("auth.json"))?, credentials);

        let refreshed = auth(&["new-refresh-model"])
            .with_model_cache_generation(newer.model_cache_generation().map(str::to_string));
        manager.persist_github_copilot_models(newer, refreshed.clone())?;
        manager.reload().await;
        assert_eq!(
            manager.auth_cached(),
            Some(CodexAuth::GitHubCopilot(refreshed))
        );
    }
    Ok(())
}

#[test]
fn refresh_cannot_restore_logged_out_credentials_or_overwrite_a_new_login() -> anyhow::Result<()> {
    let home = tempdir()?;
    let old = auth(&["old-model"]);
    let manager = AuthManager::from_auth_for_testing_with_home(
        CodexAuth::GitHubCopilot(old.clone()),
        home.path().to_path_buf(),
    );
    manager.persist_github_copilot_models(old.clone(), auth(&["new-model"]))?;
    assert!(!home.path().join("auth.json").exists());
    crate::login_with_api_key(
        home.path(),
        "replacement-key",
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;
    let replacement = std::fs::read(home.path().join("auth.json"))?;
    let current = match manager.auth_cached().unwrap() {
        CodexAuth::GitHubCopilot(auth) => auth,
        _ => unreachable!(),
    };
    manager.persist_github_copilot_models(current, auth(&["another-model"]))?;
    assert_eq!(std::fs::read(home.path().join("auth.json"))?, replacement);
    manager.set_cached_auth(Some(CodexAuth::from_api_key("replacement-key")));
    let cache = std::fs::read(home.path().join(CACHE_FILE))?;
    manager.persist_github_copilot_models(old, auth(&["stale-model"]))?;
    assert_eq!(std::fs::read(home.path().join(CACHE_FILE))?, cache);
    Ok(())
}
