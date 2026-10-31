use codex_login::GitHubCopilotAuth;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::model_catalog;

#[test]
fn copilot_limits_override_bundled_metadata_and_reserve_output_capacity() {
    for slug in ["gpt-6.1-sol", "new-copilot-model"] {
        let auth: GitHubCopilotAuth = serde_json::from_value(json!({
            "access_token": "github-test-token",
            "api_endpoint": "https://api.individual.githubcopilot.com",
            "login": null, "copilot_sku": null,
            "models": [slug],
            "model_metadata": {slug: {
                "max_context_window_tokens": 1050000,
                "limits": {"max_prompt_tokens": 922000, "max_output_tokens": 128000,
                           "parallel_tool_calls": false, "structured_outputs": true},
                "vision": false, "tool_calls": true, "streaming": true,
                "reasoning_efforts_advertised": true,
                "default_max_prompt_tokens": 272000, "long_context_max_prompt_tokens": 922000,
                "supported_endpoints": ["/responses"]
            }}
        }))
        .unwrap();
        let model = model_catalog(&auth).models.remove(/*index*/ 0);
        assert_eq!(
            (
                model.supported_reasoning_levels.clone(),
                model.default_reasoning_level.clone()
            ),
            (Vec::new(), None)
        );
        assert_eq!(
            (
                model.context_window,
                model.max_context_window,
                model.usable_context_window(),
                model.auto_compact_token_limit(),
                model.used_fallback_model_metadata,
                model.input_modalities.clone()
            ),
            (
                Some(272_000),
                Some(1_050_000),
                Some(144_000),
                Some(129_600),
                false,
                vec![codex_protocol::openai_models::InputModality::Text]
            )
        );
        for (requested, expected) in [(1_050_000, 922_000), (2_000_000, 922_000), (128_000, 0)] {
            let config = codex_models_manager::ModelsManagerConfig {
                model_context_window: Some(requested),
                model_auto_compact_token_limit: Some(i64::MAX),
                ..Default::default()
            };
            let overridden =
                codex_models_manager::model_info::with_config_overrides(model.clone(), &config);
            assert_eq!(
                (
                    overridden.usable_context_window(),
                    overridden.auto_compact_token_limit()
                ),
                (Some(expected), Some(expected * 9 / 10))
            );
        }
    }
}

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
    expected["model_metadata"] = json!({
        "gpt-6.1-terra": codex_login::GitHubCopilotModelMetadata {
            supported_endpoints: vec!["/responses".to_string()],
            reasoning_efforts_advertised: true,
            ..Default::default()
        }
    });
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
        Some(CodexAuth::GitHubCopilot(expected.clone()))
    );
    server.verify().await;
    server.reset().await;
    Mock::given(method("GET")).and(path("/models"))
        .respond_with(ResponseTemplate::new(200)
            .set_delay(std::time::Duration::from_millis(/*millis*/ 200))
            .set_body_json(json!({"data": [
                {"id": "gpt-6.1-terra", "vendor": "OpenAI", "supported_endpoints": ["/responses"],
                 "capabilities": {"limits": {"max_context_window_tokens": 500000,
                    "max_prompt_tokens": 450000, "max_output_tokens": 50000}}}
            ]})))
        .expect(1).mount(&server).await;
    assert_eq!(
        manager
            .raw_model_catalog(
                RefreshStrategy::OnlineIfUncached,
                HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault)
            )
            .await,
        model_catalog(&expected)
    );
    manager
        .raw_model_catalog(
            RefreshStrategy::OnlineIfUncached,
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        )
        .await;
    tokio::time::timeout(std::time::Duration::from_secs(/*secs*/ 5), async {
        loop {
            if auth_manager.auth_cached().is_some_and(
                |auth| matches!(auth, CodexAuth::GitHubCopilot(auth) if auth.models().len() == 1),
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(/*millis*/ 10)).await;
        }
    })
    .await?;
    assert!(provider.validate_model("gpt-6.1-sol").is_err());
    provider.validate_model("gpt-6.1-terra")?;
    let catalog = manager.get_remote_models().await;
    assert_eq!(
        (
            catalog[0].max_context_window,
            catalog[0].usable_context_window()
        ),
        (Some(500_000), Some(450_000))
    );
    assert_eq!(std::fs::read(home.path().join("auth.json"))?, credentials);
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
