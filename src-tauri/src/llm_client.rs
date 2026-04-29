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

/// Load LLM config from DB settings.
pub async fn get_llm_config_from_pool(pool: &SqlitePool) -> Result<LlmConfig, String> {
    let provider: String = db::get_setting(pool, "llm_provider")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "ollama".to_string());
    let api_key: Option<String> = db::get_setting(pool, "llm_api_key").await.ok().flatten();
    let base_url: String = db::get_setting(pool, "llm_base_url")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
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
