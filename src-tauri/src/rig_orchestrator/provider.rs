//! Map `LlmConfig` to a Rig provider client and run a single chat-completion turn.
//!
//! Rig has first-class clients for OpenAI, Anthropic, and Gemini. Ollama is
//! routed through Rig's OpenAI client by pointing `base_url` at
//! `http://host:11434/v1` (Ollama exposes an OpenAI-compatible chat completions
//! endpoint there). All four providers go through `CompletionModel::completion`,
//! so dispatch is a thin enum over Rig's per-provider `CompletionModel` type.

use crate::llm_client::{LlmConfig, LlmProvider};
use rig::completion::{CompletionError, CompletionModel, CompletionRequest};
use rig::providers::{anthropic, gemini, openai};

/// Wraps the active provider's chat-completion model. Rig's `CompletionModel`
/// trait is not dyn-compatible, so we dispatch on an enum.
pub enum RigChatModel {
    OpenAi(openai::completion::CompletionModel),
    Anthropic(anthropic::completion::CompletionModel),
    Gemini(gemini::completion::CompletionModel),
}

impl RigChatModel {
    /// Build the chat model from the existing `LlmConfig` + model name.
    /// Ollama uses the OpenAI client with `<base_url>/v1` as the base URL.
    pub fn from_config(config: &LlmConfig, model: &str) -> Result<Self, String> {
        match &config.provider {
            LlmProvider::OpenAI => {
                let key = config
                    .api_key
                    .as_deref()
                    .ok_or_else(|| "OpenAI API key not set".to_string())?;
                let mut builder = openai::Client::builder(key);
                let trimmed = config.base_url.trim_end_matches('/');
                let needs_override = !trimmed.is_empty()
                    && !trimmed.eq_ignore_ascii_case("https://api.openai.com")
                    && !trimmed.eq_ignore_ascii_case("https://api.openai.com/v1");
                if needs_override {
                    builder = builder.base_url(trimmed);
                }
                let client = builder.build();
                let responses_model = <openai::Client as rig::client::CompletionClient>::completion_model(&client, model);
                Ok(RigChatModel::OpenAi(responses_model.completions_api()))
            }
            LlmProvider::Ollama => {
                // Ollama exposes OpenAI-compat chat completions at `<base_url>/v1`.
                let trimmed = config.base_url.trim_end_matches('/');
                let oai_base = format!("{}/v1", trimmed);
                let api_key = config.api_key.as_deref().unwrap_or("ollama");
                let client = openai::Client::builder(api_key)
                    .base_url(&oai_base)
                    .build();
                let responses_model = <openai::Client as rig::client::CompletionClient>::completion_model(&client, model);
                Ok(RigChatModel::OpenAi(responses_model.completions_api()))
            }
            LlmProvider::Claude => {
                let key = config
                    .api_key
                    .as_deref()
                    .ok_or_else(|| "Claude API key not set".to_string())?;
                let client = anthropic::Client::builder(key)
                    .build()
                    .map_err(|e| format!("Anthropic client build failed: {}", e))?;
                let model = <anthropic::Client as rig::client::CompletionClient>::completion_model(&client, model);
                Ok(RigChatModel::Anthropic(model))
            }
            LlmProvider::Gemini => {
                let key = config
                    .api_key
                    .as_deref()
                    .ok_or_else(|| "Gemini API key not set".to_string())?;
                let client = gemini::Client::builder(key)
                    .build()
                    .map_err(|e| format!("Gemini client build failed: {}", e))?;
                let model = <gemini::Client as rig::client::CompletionClient>::completion_model(&client, model);
                Ok(RigChatModel::Gemini(model))
            }
        }
    }

    /// Run a single completion request. Returns `OneOrMany<AssistantContent>`
    /// flattened into a `Vec` for the orchestrator to scan.
    pub async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<Vec<rig::completion::message::AssistantContent>, CompletionError> {
        let choice = match self {
            RigChatModel::OpenAi(m) => m.completion(request).await?.choice,
            RigChatModel::Anthropic(m) => m.completion(request).await?.choice,
            RigChatModel::Gemini(m) => m.completion(request).await?.choice,
        };
        Ok(choice.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_client::{LlmConfig, LlmProvider};

    #[test]
    fn ollama_config_routes_to_openai() {
        let cfg = LlmConfig {
            provider: LlmProvider::Ollama,
            api_key: None,
            base_url: "http://localhost:11434".to_string(),
        };
        let model = RigChatModel::from_config(&cfg, "llama3.2");
        assert!(matches!(model, Ok(RigChatModel::OpenAi(_))));
    }

    #[test]
    fn openai_requires_key() {
        let cfg = LlmConfig {
            provider: LlmProvider::OpenAI,
            api_key: None,
            base_url: "https://api.openai.com".to_string(),
        };
        let err = match RigChatModel::from_config(&cfg, "gpt-4o-mini") {
            Ok(_) => panic!("expected missing-key error"),
            Err(e) => e,
        };
        assert!(err.contains("OpenAI API key"));
    }

    #[test]
    fn claude_requires_key() {
        let cfg = LlmConfig {
            provider: LlmProvider::Claude,
            api_key: None,
            base_url: String::new(),
        };
        let err = match RigChatModel::from_config(&cfg, "claude-3-5-sonnet-latest") {
            Ok(_) => panic!("expected missing-key error"),
            Err(e) => e,
        };
        assert!(err.contains("Claude API key"));
    }

    #[test]
    fn gemini_requires_key() {
        let cfg = LlmConfig {
            provider: LlmProvider::Gemini,
            api_key: None,
            base_url: String::new(),
        };
        let err = match RigChatModel::from_config(&cfg, "gemini-1.5-pro") {
            Ok(_) => panic!("expected missing-key error"),
            Err(e) => e,
        };
        assert!(err.contains("Gemini API key"));
    }
}
