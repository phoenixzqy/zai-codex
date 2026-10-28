use codex_login::GitHubCopilotAuth;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::model_catalog;

#[tokio::test]
async fn online_catalog_refresh_uses_the_mock_and_updates_the_running_provider()
-> Result<(), Box<dyn std::error::Error>> {
    use codex_http_client::HttpClientFactory;
    use codex_http_client::OutboundProxyPolicy;
    use codex_login::AuthCredentialsStoreMode;
    use codex_login::AuthKeyringBackendKind;
    use codex_login::AuthManager;
    use codex_login::CodexAuth;
    use codex_login::login_with_github_copilot;
    use codex_login::test_support::github_copilot_login_options_for_test;
    use codex_models_manager::manager::ModelsManager;
    use codex_models_manager::manager::RefreshStrategy;
    use codex_models_manager::manager::StaticModelsManager;
    use std::sync::Arc;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer github-test-token"))
        .and(header("openai-intent", "conversation"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id": "gpt-6.1-sol", "vendor": "OpenAI", "supported_endpoints": ["/responses"]},
            {"id": "gpt-6.1-terra", "vendor": "OpenAI", "supported_endpoints": ["/responses"],
             "capabilities": {"supports": {"reasoning_effort": ["low", "high"]}}},
            {"id": "disabled-model", "vendor": "OpenAI", "supported_endpoints": ["/responses"],
             "policy": {"state": "disabled"}}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir()?;
    let original = GitHubCopilotAuth::new(
        "github-test-token".to_string(),
        "https://api.individual.githubcopilot.com".to_string(),
        Some("octocat".to_string()),
        Some("copilot_individual".to_string()),
        vec!["gpt-6.1-sol".to_string()],
    )?;
    login_with_github_copilot(
        home.path(),
        original.clone(),
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    )?;
    let auth_manager = AuthManager::from_auth_for_testing_with_home(
        CodexAuth::GitHubCopilot(original.clone()),
        home.path().to_path_buf(),
    );
    auth_manager.reload().await;
    let Some(CodexAuth::GitHubCopilot(original)) = auth_manager.auth_cached() else {
        panic!("expected stored Copilot auth");
    };
    let credentials = std::fs::read(home.path().join("auth.json"))?;
    let provider = crate::provider::create_model_provider(
        codex_model_provider_info::ModelProviderInfo::create_openai_provider(
            /*base_url*/ None,
        ),
        Some(Arc::clone(&auth_manager)),
    );
    let manager = super::GitHubCopilotAwareModelsManager {
        inner: Arc::new(StaticModelsManager::new(
            Some(Arc::clone(&auth_manager)),
            model_catalog(&original),
        )),
        auth_manager: Arc::clone(&auth_manager),
        fallback_to_copilot_default_on_auth_switch: false,
        refresh_login_options: Some(github_copilot_login_options_for_test(format!(
            "{}/models",
            server.uri()
        ))?),
    };
    let factory = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault);
    assert_eq!(
        manager
            .raw_model_catalog(RefreshStrategy::Offline, factory.clone())
            .await,
        model_catalog(&original)
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(provider.validate_model("gpt-6.1-terra").is_err());

    let catalog = manager
        .raw_model_catalog(RefreshStrategy::Online, factory.clone())
        .await;
    let mut expected = serde_json::to_value(original)?;
    expected["models"] = json!(["gpt-6.1-sol", "gpt-6.1-terra"]);
    expected["model_reasoning_efforts"] = json!({"gpt-6.1-terra": ["low", "high"]});
    let expected: GitHubCopilotAuth = serde_json::from_value(expected)?;
    assert_eq!(catalog, model_catalog(&expected));
    assert_eq!(
        auth_manager.auth_cached(),
        Some(CodexAuth::GitHubCopilot(expected.clone()))
    );
    provider.validate_model("gpt-6.1-terra")?;
    assert!(provider.validate_model("disabled-model").is_err());
    assert_eq!(
        manager
            .raw_model_catalog(RefreshStrategy::Offline, factory)
            .await,
        catalog
    );
    assert!(home.path().join("github_copilot_models.json").is_file());
    assert_eq!(std::fs::read(home.path().join("auth.json"))?, credentials);
    auth_manager.reload().await;
    assert_eq!(
        auth_manager.auth_cached(),
        Some(CodexAuth::GitHubCopilot(expected))
    );
    server.verify().await;
    Ok(())
}

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
