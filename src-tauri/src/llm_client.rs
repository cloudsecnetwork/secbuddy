//! LLM client: Claude, OpenAI, Ollama, Gemini behind a single interface. Tool/function calling and streaming.

use crate::db;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ToolCall {
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LlmResponse {
    pub choices: Option<Vec<Choice>>,
    pub message: Option<ChatMessage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Choice {
    pub message: Option<ChatMessage>,
    pub delta: Option<Delta>,
    pub index: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Delta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeltaToolCall {
    pub index: Option<u32>,
    pub id: Option<String>,
    pub function: Option<DeltaFunction>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeltaFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// Result of one LLM call: assistant text and any tool calls.
pub struct LlmCallResult {
    pub content: String,
    pub tool_calls: Vec<ParsedToolCall>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ParsedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
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

/// Load LLM config from DB settings (shared by agent loop and test_connection).
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

/// Call Ollama chat completion with optional tools. Returns content and parsed tool calls.
pub async fn ollama_chat(
    base_url: &str,
    model: &str,
    messages: &[Value],
    tools_json: Option<&[Value]>,
) -> Result<LlmCallResult, String> {
    let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false
    });
    if let Some(tools) = tools_json {
        body["tools"] = serde_json::to_value(tools).map_err(|e| e.to_string())?;
    }
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {}", e, text))?;
    let message = v
        .get("message")
        .ok_or_else(|| "No message in Ollama response".to_string())?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    let func = tc.get("function")?;
                    let name = func.get("name").and_then(Value::as_str)?.to_string();
                    let args = match func.get("arguments") {
                        Some(Value::String(s)) => s.clone(),
                        Some(obj @ Value::Object(_)) => serde_json::to_string(obj).unwrap_or_else(|_| "{}".to_string()),
                        _ => "{}".to_string(),
                    };
                    Some(ParsedToolCall { id, name, arguments: args })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(LlmCallResult {
        content,
        tool_calls,
    })
}

/// Call OpenAI-compatible chat (OpenAI or Claude via API) with tools. Non-streaming.
pub async fn openai_chat(
    provider: &LlmProvider,
    api_key: &str,
    model: &str,
    messages: &[Value],
    tools_json: &[Value],
) -> Result<LlmCallResult, String> {
    let (url, headers) = match provider {
        LlmProvider::OpenAI => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            vec![("Authorization", format!("Bearer {}", api_key))],
        ),
        LlmProvider::Claude => {
            return Err("Claude chat completion not implemented in this path; use Claude API format".to_string());
        }
        _ => return Err("Not an API provider".to_string()),
    };
    let mut req = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "tools": tools_json,
        "stream": false
    });
    let resp = req.json(&body).send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {}", e, text))?;
    let choices = v.get("choices").and_then(Value::as_array).ok_or("No choices")?;
    let choice = choices.first().ok_or("Empty choices")?;
    let message = choice.get("message").ok_or("No message")?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    let func = tc.get("function")?;
                    let name = func.get("name").and_then(Value::as_str)?.to_string();
                    let args = match func.get("arguments") {
                        Some(Value::String(s)) => s.clone(),
                        Some(obj @ Value::Object(_)) => serde_json::to_string(obj).unwrap_or_else(|_| "{}".to_string()),
                        _ => "{}".to_string(),
                    };
                    Some(ParsedToolCall { id, name, arguments: args })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(LlmCallResult {
        content,
        tool_calls,
    })
}

/// Convert OpenAI-style tools to Gemini functionDeclarations.
fn openai_tools_to_gemini(tools_json: &[Value]) -> Vec<Value> {
    tools_json
        .iter()
        .filter_map(|t| {
            let func = t.get("function")?;
            let name = func.get("name")?.as_str()?;
            let description = func.get("description").and_then(Value::as_str).unwrap_or("").to_string();
            let parameters = func.get("parameters").cloned().unwrap_or(serde_json::json!({ "type": "object", "properties": {} }));
            Some(serde_json::json!({
                "name": name,
                "description": description,
                "parameters": parameters
            }))
        })
        .collect()
}

/// Convert OpenAI-style messages to Gemini contents (user/model with parts[].text).
fn openai_messages_to_gemini_contents(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .filter_map(|m| {
            let role = m.get("role").and_then(Value::as_str)?;
            let content = m.get("content").and_then(Value::as_str).unwrap_or("").to_string();
            let gemini_role = if role == "assistant" { "model" } else { "user" };
            Some(serde_json::json!({
                "role": gemini_role,
                "parts": [{ "text": content }]
            }))
        })
        .collect()
}

/// Call Gemini generateContent with tools. Returns content and parsed tool calls.
pub async fn gemini_chat(
    api_key: &str,
    model: &str,
    messages: &[Value],
    tools_json: &[Value],
) -> Result<LlmCallResult, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );
    let contents = openai_messages_to_gemini_contents(messages);
    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": { "temperature": 0.2 }
    });
    if !tools_json.is_empty() {
        let declarations = openai_tools_to_gemini(tools_json);
        body["tools"] = serde_json::json!([{ "functionDeclarations": declarations }]);
    }
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "Gemini returned {}: {}",
            status,
            text.chars().take(500).collect::<String>()
        ));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {}", e, text))?;
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(Value::as_str).unwrap_or("unknown");
        return Err(format!("Gemini API error: {}", msg));
    }
    let candidates = v.get("candidates").and_then(Value::as_array).ok_or_else(|| {
        let reason = v
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|c| c.get("finishReason").and_then(Value::as_str))
            .unwrap_or("unknown");
        format!("No candidates in Gemini response (finishReason: {})", reason)
    })?;
    let candidate = candidates.first().ok_or("Empty candidates")?;
    let content_obj = candidate.get("content").ok_or("No content in candidate")?;
    let parts = content_obj.get("parts").and_then(Value::as_array).ok_or("No parts in content")?;
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        if let Some(t) = part.get("text").and_then(Value::as_str) {
            content.push_str(t);
        }
        if let Some(fc) = part.get("functionCall") {
            let name = fc.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let args_val = fc.get("args");
            let arguments = if let Some(args) = args_val {
                serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            };
            let id = format!("gemini-fc-{}", idx);
            tool_calls.push(ParsedToolCall {
                id,
                name,
                arguments,
            });
        }
    }
    Ok(LlmCallResult {
        content,
        tool_calls,
    })
}

/// Single entry point: call LLM with config, messages, and tools. Returns content + tool calls.
pub async fn chat(
    config: &LlmConfig,
    model: &str,
    messages: &[Value],
    tools_json: &[Value],
) -> Result<LlmCallResult, String> {
    match &config.provider {
        LlmProvider::Ollama => ollama_chat(
            &config.base_url,
            model,
            messages,
            if tools_json.is_empty() { None } else { Some(tools_json) },
        )
        .await,
        LlmProvider::OpenAI => {
            let key = config.api_key.as_deref().ok_or("OpenAI API key not set")?;
            openai_chat(&config.provider, key, model, messages, tools_json).await
        }
        LlmProvider::Claude => {
            let _key = config.api_key.as_deref().ok_or("Claude API key not set")?;
            Err("Claude chat with tools: use OpenAI-compatible endpoint or implement".to_string())
        }
        LlmProvider::Gemini => {
            let key = config.api_key.as_deref().ok_or("Gemini API key not set")?;
            gemini_chat(key, model, messages, tools_json).await
        }
    }
}
