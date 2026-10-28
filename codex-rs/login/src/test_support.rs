//! Test-only helpers exposed for cross-crate integration tests.
//!
//! Production code should receive an [`AuthRouteConfig`](crate::AuthRouteConfig) adapted from the
//! application's resolved HTTP client factory instead of depending on this module.

use crate::AuthManager;
use crate::AuthRouteConfig;
use crate::CodexAuth;
use crate::auth::AgentIdentityAuth;
use crate::auth::AgentIdentityAuthRecord;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use codex_protocol::account::PlanType as AccountPlanType;
use std::sync::Arc;

/// Creates an authentication manager with optional credentials for cross-crate tests.
pub fn auth_manager_from_optional_auth(auth: Option<CodexAuth>) -> Arc<AuthManager> {
    AuthManager::from_optional_auth_for_testing(auth)
}

/// Creates an authentication manager with initialized Agent Identity credentials for tests.
pub async fn auth_manager_with_agent_identity() -> std::io::Result<Arc<AuthManager>> {
    let key_material =
        codex_agent_identity::generate_agent_key_material().map_err(std::io::Error::other)?;
    let auth = AgentIdentityAuth::from_record(
        AgentIdentityAuthRecord {
            agent_runtime_id: "test-agent-runtime-id".to_string(),
            agent_private_key: key_material.private_key_pkcs8_base64,
            account_id: "test-account-id".to_string(),
            chatgpt_user_id: "test-user-id".to_string(),
            email: Some("test-user@example.com".to_string()),
            plan_type: AccountPlanType::Pro,
            chatgpt_account_is_fedramp: false,
            task_id: Some("test-task-id".to_string()),
        },
        "https://unused.invalid",
        &transport_default_auth_route_config(),
    )
    .await?;
    Ok(auth_manager_from_optional_auth(Some(
        CodexAuth::AgentIdentity(auth),
    )))
}

/// Returns auth routing that preserves the transport's built-in proxy behavior.
pub fn transport_default_auth_route_config() -> AuthRouteConfig {
    AuthRouteConfig::from_http_client_factory(HttpClientFactory::new(
        OutboundProxyPolicy::ReqwestDefault,
    ))
}

/// Creates Copilot refresh options that can send requests only to an IP-loopback fixture.
pub fn github_copilot_login_options_for_test(
    models_url: String,
) -> std::io::Result<crate::GitHubCopilotLoginOptions> {
    let url = url::Url::parse(&models_url).map_err(std::io::Error::other)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.host_str().is_some_and(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
        })
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Copilot test model URL must use an IP-loopback HTTP(S) endpoint",
        ));
    }
    let controller = codex_http_client::NetworkPolicyController::default();
    let policy = controller.policy();
    controller.publish(
        policy.revision(),
        codex_http_client::DestinationPolicy::Unrestricted,
    );
    let factory = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault)
        .with_network_policy(policy.restrict_to_endpoints([url].into()));
    let mut options = crate::GitHubCopilotLoginOptions::new(
        crate::GITHUB_COPILOT_DEFAULT_CLIENT_ID.to_string(),
        AuthRouteConfig::from_http_client_factory(factory),
    )?;
    options.models_url_override = Some(models_url);
    Ok(options)
}
