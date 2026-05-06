//! LLM provider configuration shared by the rig orchestrator and the
//! "Test connection" Tauri command. Actual chat completions are issued by
//! `rig_orchestrator::provider::RigChatModel` against `rig::providers::*`.

use crate::db;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Claude,
    OpenAI,
    Ollama,
    Gemini,
}

#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub api_key: Option<String>,
    pub base_url: String, // for Ollama, e.g. http://localhost:11434
}

impl LlmConfig {
    #[allow(dead_code)]
    pub fn ollama_default() -> Self {
        Self {
            provider: LlmProvider::Ollama,
            api_key: None,
            base_url: "http://localhost:11434".to_string(),
        }
    }
}

const OLLAMA_DEFAULT_BASE: &str = "http://localhost:11434";

fn is_default_ollama_base(trimmed: &str) -> bool {
    let root = trimmed.trim_end_matches('/');
    let lower = root.to_ascii_lowercase();
    lower == "http://localhost:11434" || lower == "http://127.0.0.1:11434"
}

/// Resolve `llm_base_url` for the active provider so cloud providers do not inherit
/// Ollama's localhost default from the DB or from switching provider in the UI.
fn normalized_llm_base_url(provider: &str, stored: Option<String>) -> String {
    let raw = stored.unwrap_or_default();
    let trim = raw.trim();
    match provider.to_lowercase().as_str() {
        "openai" | "claude" | "gemini" => {
            if trim.is_empty() || is_default_ollama_base(trim) {
                String::new()
            } else {
                trim.to_string()
            }
        }
        _ => {
            if trim.is_empty() {
                OLLAMA_DEFAULT_BASE.to_string()
            } else {
                trim.to_string()
            }
        }
    }
}

/// Load LLM config from DB settings.
pub async fn get_llm_config_from_pool(pool: &SqlitePool) -> Result<LlmConfig, String> {
    let provider: String = db::get_setting(pool, "llm_provider")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "ollama".to_string());
    let api_key: Option<String> = db::get_setting(pool, "llm_api_key").await.ok().flatten();
    let stored_base = db::get_setting(pool, "llm_base_url").await.ok().flatten();
    let base_url = normalized_llm_base_url(&provider, stored_base);
    let config = match provider.to_lowercase().as_str() {
        "claude" => LlmConfig {
            provider: LlmProvider::Claude,
            api_key,
            base_url,
        },
        "openai" => LlmConfig {
            provider: LlmProvider::OpenAI,
            api_key,
            base_url,
        },
        "gemini" => LlmConfig {
            provider: LlmProvider::Gemini,
            api_key,
            base_url,
        },
        _ => LlmConfig {
            provider: LlmProvider::Ollama,
            api_key,
            base_url,
        },
    };
    Ok(config)
}

/// Test connection (e.g. list models for Ollama or minimal completion for API).
pub async fn test_connection(config: &LlmConfig) -> Result<(), String> {
    match &config.provider {
        LlmProvider::Ollama => {
            let url = format!("{}/api/tags", config.base_url.trim_end_matches('/'));
            let resp = reqwest::Client::new()
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("Ollama returned {}", resp.status()));
            }
            Ok(())
        }
        LlmProvider::OpenAI => {
            let key = config.api_key.as_deref().ok_or("OpenAI API key not set")?;
            let resp = reqwest::Client::new()
                .get("https://api.openai.com/v1/models")
                .header("Authorization", format!("Bearer {}", key))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("OpenAI returned {}", resp.status()));
            }
            Ok(())
        }
        LlmProvider::Claude => {
            let key = config.api_key.as_deref().ok_or("Claude API key not set")?;
            let resp = reqwest::Client::new()
                .get("https://api.anthropic.com/v1/models")
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("Claude returned {}", resp.status()));
            }
            Ok(())
        }
        LlmProvider::Gemini => {
            let key = config.api_key.as_deref().ok_or("Gemini API key not set")?;
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                key
            );
            let resp = reqwest::Client::new()
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("Gemini returned {}", resp.status()));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod normalized_base_url_tests {
    use super::normalized_llm_base_url;

    #[test]
    fn ollama_missing_uses_default() {
        assert_eq!(
            normalized_llm_base_url("ollama", None),
            "http://localhost:11434"
        );
    }

    #[test]
    fn openai_missing_is_empty() {
        assert_eq!(normalized_llm_base_url("openai", None), "");
    }

    #[test]
    fn openai_stale_ollama_host_is_cleared() {
        assert_eq!(
            normalized_llm_base_url(
                "openai",
                Some("http://localhost:11434".to_string())
            ),
            ""
        );
        assert_eq!(
            normalized_llm_base_url(
                "openai",
                Some("http://127.0.0.1:11434/".to_string())
            ),
            ""
        );
    }

    #[test]
    fn openai_custom_proxy_kept() {
        assert_eq!(
            normalized_llm_base_url("openai", Some("https://example.com/v1".to_string())),
            "https://example.com/v1"
        );
    }
}
