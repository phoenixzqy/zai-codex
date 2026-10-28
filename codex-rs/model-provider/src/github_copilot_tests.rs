use codex_login::GitHubCopilotAuth;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::model_catalog;

#[tokio::test]
async fn running_provider_accepts_refreshed_models_but_rejects_a_different_credential()
-> Result<(), Box<dyn std::error::Error>> {
    use codex_login::AuthCredentialsStoreMode;
    use codex_login::AuthKeyringBackendKind;
    use codex_login::AuthManager;
    use codex_login::CodexAuth;
    use codex_login::login_with_github_copilot;
    use std::sync::Arc;

    let home = tempfile::tempdir()?;
    let original = GitHubCopilotAuth::new(
        "github-test-token".to_string(),
        "https://api.individual.githubcopilot.com".to_string(),
        Some("octocat".to_string()),
        Some("copilot_individual".to_string()),
        vec!["gpt-6.1-sol".to_string()],
    )?;
    let manager = AuthManager::from_auth_for_testing_with_home(
        CodexAuth::GitHubCopilot(original.clone()),
        home.path().to_path_buf(),
    );
    let provider = crate::provider::create_model_provider(
        codex_model_provider_info::ModelProviderInfo::create_openai_provider(
            /*base_url*/ None,
        ),
        Some(Arc::clone(&manager)),
    );
    assert!(provider.validate_model("gpt-6.1-terra").is_err());

    let mut value = serde_json::to_value(original)?;
    value["models"] = json!(["gpt-6.1-sol", "gpt-6.1-terra"]);
    login_with_github_copilot(
        home.path(),
        serde_json::from_value(value.clone())?,
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;
    manager.reload().await;
    provider.validate_model("gpt-6.1-terra")?;
    value["access_token"] = json!("different-token");
    login_with_github_copilot(
        home.path(),
        serde_json::from_value(value)?,
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;
    manager.reload().await;
    assert!(provider.validate_model("gpt-6.1-terra").is_err());
    Ok(())
}

#[test]
fn catalog_uses_copilot_reasoning_efforts_for_unknown_models() {
    let auth = serde_json::from_value::<GitHubCopilotAuth>(json!({
        "access_token": "github-token",
        "api_endpoint": "https://api.individual.githubcopilot.com",
        "login": "octocat",
        "copilot_sku": "copilot_individual",
        "models": ["unknown-copilot-test-model"],
        "model_reasoning_efforts": {
            "unknown-copilot-test-model": ["low", "medium", "high", "xhigh", "max"]
        }
    }))
    .expect("GitHub Copilot auth fixture should deserialize");

    let catalog = model_catalog(&auth);
    let model = catalog.models.first().expect("Astra should be available");

    assert_eq!(model.slug, "unknown-copilot-test-model");
    assert_eq!(model.default_reasoning_level, Some(ReasoningEffort::Medium));
    assert_eq!(
        model
            .supported_reasoning_levels
            .iter()
            .map(|preset| preset.effort.clone())
            .collect::<Vec<_>>(),
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
            ReasoningEffort::Max,
        ]
    );
}

#[test]
fn catalog_preserves_local_ultra_reasoning_for_bundled_models() {
    let auth = serde_json::from_value::<GitHubCopilotAuth>(json!({
        "access_token": "github-token",
        "api_endpoint": "https://api.individual.githubcopilot.com",
        "login": "octocat",
        "copilot_sku": "copilot_individual",
        "models": ["gpt-5.6-sol"],
        "model_reasoning_efforts": {
            "gpt-5.6-sol": ["none", "low", "medium", "high", "xhigh", "max"]
        }
    }))
    .expect("GitHub Copilot auth fixture should deserialize");

    let catalog = model_catalog(&auth);
    let model = catalog.models.first().expect("Sol should be available");

    assert_eq!(model.default_reasoning_level, Some(ReasoningEffort::Low));
    assert_eq!(
        model
            .supported_reasoning_levels
            .iter()
            .map(|preset| preset.effort.clone())
            .collect::<Vec<_>>(),
        vec![
            ReasoningEffort::None,
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
            ReasoningEffort::Max,
            ReasoningEffort::Ultra,
        ]
    );
}
