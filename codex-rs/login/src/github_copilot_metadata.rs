use codex_protocol::openai_models::ProviderModelLimits;
use serde::Deserialize;
use serde::Serialize;

/// Account-scoped Copilot capabilities, independent of the saved credential identity.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct GitHubCopilotModelMetadata {
    pub max_context_window_tokens: Option<i64>,
    pub limits: ProviderModelLimits,
    pub vision: Option<bool>,
    pub tool_calls: Option<bool>,
    pub streaming: Option<bool>,
    #[serde(default)]
    pub reasoning_efforts_advertised: bool,
    pub default_max_prompt_tokens: Option<i64>,
    pub long_context_max_prompt_tokens: Option<i64>,
    pub supported_endpoints: Vec<String>,
}

impl GitHubCopilotModelMetadata {
    pub(crate) fn validate(&self) -> std::io::Result<()> {
        let limits = [
            self.max_context_window_tokens,
            self.limits.max_prompt_tokens,
            self.limits.max_output_tokens,
            self.default_max_prompt_tokens,
            self.long_context_max_prompt_tokens,
        ];
        if limits.into_iter().flatten().any(|limit| limit <= 0)
            || matches!(
                (self.max_context_window_tokens, self.limits.max_output_tokens),
                (Some(context), Some(output)) if output >= context
            )
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "GitHub Copilot advertised invalid model token limits",
            ));
        }
        Ok(())
    }
}
