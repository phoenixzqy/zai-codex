use anyhow::Result;
use codex_core::TurnInputRequest;
use codex_login::CodexAuth;
use codex_login::GitHubCopilotAuth;
use codex_protocol::protocol::EventMsg;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::start_mock_server;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;

#[tokio::test]
async fn provider_prompt_and_generation_limits_reach_inference() -> Result<()> {
    use codex_protocol::openai_models::ProviderModelLimits;
    use core_test_support::responses::ev_completed;
    use core_test_support::responses::ev_response_created;
    use core_test_support::responses::mount_sse_once;
    use core_test_support::responses::sse;
    use pretty_assertions::assert_eq;

    let server = start_mock_server().await;
    let mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("provider-limits"),
            ev_completed("provider-limits"),
        ]),
    )
    .await;
    let test = test_codex()
        .with_model("gpt-5.4")
        .with_model_info_override("gpt-5.4", |model| {
            model.context_window = Some(1_050_000);
            model.max_context_window = Some(1_050_000);
            model.provider_limits = Some(ProviderModelLimits {
                max_prompt_tokens: Some(922_000),
                max_output_tokens: Some(128_000),
                parallel_tool_calls: Some(false),
                structured_outputs: Some(true),
            });
        })
        .build_with_auto_env(&server)
        .await?;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Summarize this neutral synthetic history.".repeat(/*n*/ 20_000),
            text_elements: Vec::new(),
        }]))
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    let body = mock.single_request().body_json();
    assert_eq!(
        (
            body["max_output_tokens"].clone(),
            body["parallel_tool_calls"].clone()
        ),
        (serde_json::json!(128000), serde_json::json!(false))
    );
    Ok(())
}

#[tokio::test]
async fn github_copilot_auth_rejects_unadvertised_model_without_network_fallback() -> Result<()> {
    let server = start_mock_server().await;
    let auth = GitHubCopilotAuth::new(
        "github-token".to_string(),
        "https://api.individual.githubcopilot.com".to_string(),
        Some("octocat".to_string()),
        Some("copilot_individual".to_string()),
        vec!["gpt-5.6-sol".to_string()],
    )?;
    let test = test_codex()
        .with_auth(CodexAuth::from_github_copilot(auth))
        .with_model("not-advertised-by-copilot")
        .build_with_auto_env(&server)
        .await?;

    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "hello".to_string(),
            text_elements: Vec::new(),
        }]))
        .await?;

    let event = wait_for_event(&test.codex, |event| matches!(event, EventMsg::Error(_))).await;
    let EventMsg::Error(error) = event else {
        unreachable!("wait_for_event returned a non-error event")
    };
    assert!(
        error
            .message
            .contains("model `not-advertised-by-copilot` is not an enabled OpenAI Responses model"),
        "unexpected error: {}",
        error.message
    );
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}
