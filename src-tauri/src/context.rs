//! Hierarchical State-Aware Memory: Battle Map, Sliding Window, Log-Dump truncation.
//!
//! Tier 1 (Battle Map): structured state extracted from tool outputs, pinned at prompt top.
//! Tier 2 (Sliding Window): only the last N messages sent to LLM, with orphan prevention.
//! Tier 3 (Log-Dump): large tool outputs truncated to first M lines / C chars for the LLM.

use crate::db;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub const WINDOW_SIZE: usize = 10;
const MAX_TOOL_OUTPUT_CHARS: usize = 1500;
const MAX_TOOL_SUMMARY_LINES: usize = 30;

// ---------------------------------------------------------------------------
// Battle Map (Tier 1)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BattleMap {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub os_info: Option<String>,
    #[serde(default)]
    pub open_ports: Vec<String>,
    #[serde(default)]
    pub dns_records: Vec<String>,
    #[serde(default)]
    pub tls_info: Option<String>,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub findings_summary: Vec<String>,
    #[serde(default)]
    pub skipped_tools: Vec<String>,
    #[serde(default)]
    pub current_goal: Option<String>,
}

/// Render battle map into a deterministic XML block for the system prompt.
/// Field order is fixed; empty fields produce stable placeholders for cache hits.
pub fn render_battle_map(map: &BattleMap) -> String {
    let target = xml_escape(map.target.as_deref().unwrap_or("unknown"));
    let os = xml_escape(map.os_info.as_deref().unwrap_or("unknown"));

    let ports = if map.open_ports.is_empty() {
        "none".to_string()
    } else {
        map.open_ports
            .iter()
            .map(|p| format!("- {}", xml_escape(p)))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let dns = if map.dns_records.is_empty() {
        "none".to_string()
    } else {
        map.dns_records.iter().map(|r| xml_escape(r)).collect::<Vec<_>>().join(", ")
    };

    let tls = xml_escape(map.tls_info.as_deref().unwrap_or("none"));

    let findings = if map.findings_summary.is_empty() {
        "none".to_string()
    } else {
        map.findings_summary.iter().map(|f| xml_escape(f)).collect::<Vec<_>>().join(", ")
    };

    let skipped = if map.skipped_tools.is_empty() {
        "none".to_string()
    } else {
        map.skipped_tools.join(", ")
    };

    let goal = xml_escape(map.current_goal.as_deref().unwrap_or("awaiting user request"));

    format!(
        "<battle_map>\n\
         <target>{target}</target>\n\
         <os>{os}</os>\n\
         <ports>\n{ports}\n</ports>\n\
         <dns>{dns}</dns>\n\
         <tls>{tls}</tls>\n\
         <findings>{findings}</findings>\n\
         <skipped_tools>{skipped}</skipped_tools>\n\
         <goal>{goal}</goal>\n\
         </battle_map>"
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Load battle map from DB, falling back to empty for new chats.
pub async fn build_battle_map_from_db(
    pool: &SqlitePool,
    chat_id: &str,
) -> Result<BattleMap, String> {
    let json_str = db::get_battle_map(pool, chat_id)
        .await
        .map_err(|e| e.to_string())?;
    match json_str {
        Some(s) if !s.is_empty() => {
            serde_json::from_str(&s).map_err(|e| format!("battle_map deserialize: {}", e))
        }
        _ => Ok(BattleMap::default()),
    }
}

/// Persist battle map JSON to DB.
async fn save_battle_map(
    pool: &SqlitePool,
    chat_id: &str,
    map: &BattleMap,
) -> Result<(), String> {
    let json_str = serde_json::to_string(map).map_err(|e| e.to_string())?;
    db::update_battle_map(pool, chat_id, &json_str)
        .await
        .map_err(|e| e.to_string())
}

/// Update battle map after a tool execution. Parses output heuristically (no LLM call).
pub async fn update_battle_map(
    pool: &SqlitePool,
    chat_id: &str,
    tool_name: &str,
    target: &str,
    raw_output: &str,
) -> Result<(), String> {
    let mut map = build_battle_map_from_db(pool, chat_id).await?;

    if map.target.is_none() && !target.is_empty() {
        map.target = Some(target.to_string());
    }

    match tool_name {
        "nmap" => parse_nmap_into_map(&mut map, raw_output),
        "dig" | "nslookup" | "host" => parse_dig_into_map(&mut map, raw_output),
        "curl" | "httpie" => parse_curl_into_map(&mut map, raw_output),
        "whois" => parse_whois_into_map(&mut map, raw_output),
        "openssl" | "testssl" | "sslscan" => parse_tls_into_map(&mut map, raw_output),
        "subfinder" | "amass" | "theharvester" => parse_subdomain_into_map(&mut map, raw_output),
        _ => {}
    }

    save_battle_map(pool, chat_id, &map).await
}

/// Record a finding in the battle map summary.
pub async fn add_finding_to_battle_map(
    pool: &SqlitePool,
    chat_id: &str,
    title: &str,
    severity: &str,
) -> Result<(), String> {
    let mut map = build_battle_map_from_db(pool, chat_id).await?;
    let entry = format!("{} ({})", title, severity);
    if !map.findings_summary.contains(&entry) {
        map.findings_summary.push(entry);
    }
    save_battle_map(pool, chat_id, &map).await
}

/// Mark a tool as skipped (dry_run / denied) in the battle map.
pub async fn mark_tool_skipped(
    pool: &SqlitePool,
    chat_id: &str,
    tool_name: &str,
) -> Result<(), String> {
    let mut map = build_battle_map_from_db(pool, chat_id).await?;
    let name = tool_name.to_string();
    if !map.skipped_tools.contains(&name) {
        map.skipped_tools.push(name);
    }
    save_battle_map(pool, chat_id, &map).await
}

/// Set the current goal from the first user message.
pub async fn set_goal_if_empty(
    pool: &SqlitePool,
    chat_id: &str,
    user_message: &str,
) -> Result<(), String> {
    let mut map = build_battle_map_from_db(pool, chat_id).await?;
    if map.current_goal.is_none() {
        let goal = if user_message.len() > 200 {
            let end = floor_char_boundary(user_message, 200);
            format!("{}...", &user_message[..end])
        } else {
            user_message.to_string()
        };
        map.current_goal = Some(goal);
        save_battle_map(pool, chat_id, &map).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tool output parsers (heuristic, no LLM calls)
// ---------------------------------------------------------------------------

fn parse_nmap_into_map(map: &mut BattleMap, output: &str) {
    let port_re = Regex::new(r"(\d+)/(tcp|udp)\s+open\s+(\S+)\s*(.*)").unwrap();
    for cap in port_re.captures_iter(output) {
        let port = cap[1].to_string();
        let proto = &cap[2];
        let service = cap[3].to_string();
        let version = cap[4].trim().to_string();
        let entry = if version.is_empty() {
            format!("{}/{} {}", port, proto, service)
        } else {
            format!("{}/{} {} {}", port, proto, service, version)
        };
        if !map.open_ports.contains(&entry) {
            map.open_ports.push(entry);
        }
        if !map.services.contains(&service) {
            map.services.push(service);
        }
    }

    let os_re = Regex::new(r"(?i)OS details?:\s*(.+)").unwrap();
    if let Some(cap) = os_re.captures(output) {
        map.os_info = Some(cap[1].trim().to_string());
    }
    if map.os_info.is_none() {
        let aggressive_os_re = Regex::new(r"(?i)Running:\s*(.+)").unwrap();
        if let Some(cap) = aggressive_os_re.captures(output) {
            map.os_info = Some(cap[1].trim().to_string());
        }
    }
}

fn parse_dig_into_map(map: &mut BattleMap, output: &str) {
    let record_re = Regex::new(r"(?m)^\S+\.\s+\d+\s+IN\s+(A|AAAA|MX|NS|CNAME|TXT)\s+(.+)$").unwrap();
    for cap in record_re.captures_iter(output) {
        let rtype = &cap[1];
        let value = cap[2].trim().to_string();
        let entry = format!("{}={}", rtype, value);
        if !map.dns_records.contains(&entry) {
            map.dns_records.push(entry);
        }
    }
}

fn parse_curl_into_map(map: &mut BattleMap, output: &str) {
    let status_re = Regex::new(r"(?m)^HTTP/[\d.]+\s+(\d{3}\s*.*)$").unwrap();
    if let Some(cap) = status_re.captures(output) {
        let status = format!("HTTP {}", cap[1].trim());
        if !map.services.contains(&status) {
            map.services.push(status);
        }
    }

    let server_re = Regex::new(r"(?im)^[Ss]erver:\s*(.+)$").unwrap();
    if let Some(cap) = server_re.captures(output) {
        let server = cap[1].trim().to_string();
        if !map.services.contains(&server) {
            map.services.push(server);
        }
    }
}

fn parse_whois_into_map(map: &mut BattleMap, output: &str) {
    let org_re = Regex::new(r"(?im)^(?:Registrant\s+)?Organi[sz]ation:\s*(.+)$").unwrap();
    if let Some(cap) = org_re.captures(output) {
        let org = cap[1].trim().to_string();
        if !map.dns_records.contains(&format!("Org={}", org)) {
            map.dns_records.push(format!("Org={}", org));
        }
    }

    let creation_re = Regex::new(r"(?im)^(?:Creation|Created)\s*(?:Date)?\s*[=:]\s*(.+)$").unwrap();
    if let Some(cap) = creation_re.captures(output) {
        let entry = format!("Created={}", cap[1].trim());
        if !map.dns_records.contains(&entry) {
            map.dns_records.push(entry);
        }
    }

    let expiry_re = Regex::new(r"(?im)^(?:Registry\s+)?Expir(?:y|ation)\s*(?:Date)?\s*[=:]\s*(.+)$").unwrap();
    if let Some(cap) = expiry_re.captures(output) {
        let entry = format!("Expires={}", cap[1].trim());
        if !map.dns_records.contains(&entry) {
            map.dns_records.push(entry);
        }
    }
}

fn parse_tls_into_map(map: &mut BattleMap, output: &str) {
    let mut parts: Vec<String> = Vec::new();

    let cn_re = Regex::new(r"(?i)(?:subject|CN)\s*[=:]\s*(?:.*CN\s*=\s*)?([^\s/,]+)").unwrap();
    if let Some(cap) = cn_re.captures(output) {
        parts.push(format!("CN={}", cap[1].trim()));
    }

    let expiry_re = Regex::new(r"(?i)(?:Not After|notAfter|expires?)\s*[=:]\s*(.+)").unwrap();
    if let Some(cap) = expiry_re.captures(output) {
        let expiry = cap[1].trim();
        let short = if expiry.len() > 30 { &expiry[..floor_char_boundary(expiry, 30)] } else { expiry };
        parts.push(format!("expires={}", short));
    }

    let issuer_re = Regex::new(r"(?i)issuer\s*[=:]\s*(?:.*(?:CN|O)\s*=\s*)?([^\s/,]+)").unwrap();
    if let Some(cap) = issuer_re.captures(output) {
        parts.push(format!("issuer={}", cap[1].trim()));
    }

    if !parts.is_empty() {
        map.tls_info = Some(parts.join(", "));
    }
}

fn parse_subdomain_into_map(map: &mut BattleMap, output: &str) {
    let subdomain_re = Regex::new(r"(?m)^([a-zA-Z0-9][-a-zA-Z0-9]*\.)+[a-zA-Z]{2,}$").unwrap();
    let mut count = 0;
    for m in subdomain_re.find_iter(output) {
        let sub = m.as_str().to_string();
        let entry = format!("SUB={}", sub);
        if !map.dns_records.contains(&entry) {
            map.dns_records.push(entry);
            count += 1;
            if count >= 20 {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Log-Dump Truncation (Tier 3)
// ---------------------------------------------------------------------------

/// Truncate a tool output for LLM consumption. Applies dual cap: line count AND char count.
/// Returns the original content if it's already under threshold.
fn truncate_tool_content(content: &str) -> String {
    if content.len() <= MAX_TOOL_OUTPUT_CHARS {
        return content.to_string();
    }

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let total_chars = content.len();

    let kept_lines: Vec<&str> = lines.into_iter().take(MAX_TOOL_SUMMARY_LINES).collect();
    let mut summary = kept_lines.join("\n");

    if summary.len() > MAX_TOOL_OUTPUT_CHARS {
        let end = floor_char_boundary(&summary, MAX_TOOL_OUTPUT_CHARS);
        summary.truncate(end);
    }

    format!(
        "{}\n[... truncated: {} lines / {} chars total. Showing first {}. Full output saved locally.]",
        summary,
        total_lines,
        total_chars,
        MAX_TOOL_SUMMARY_LINES.min(total_lines),
    )
}

// ---------------------------------------------------------------------------
// Sliding Window + Mission Start (Tier 2)
// ---------------------------------------------------------------------------

/// Message tuple from DB: (id, chat_id, role, content, tool_invocation_id, created_at).
pub type DbMessage = (String, String, String, String, Option<String>, i64);

/// Build the LLM API messages array with:
/// 1. System prompt (with battle map prepended)
/// 2. Mission Start (first user message, always pinned)
/// 3. Sliding window of last N messages with orphan prevention
/// 4. Tool outputs truncated if over threshold
pub fn build_api_messages(
    system_content: &str,
    all_messages: &[DbMessage],
    window_size: usize,
) -> Vec<Value> {
    let mut api_messages: Vec<Value> = Vec::new();

    api_messages.push(json!({
        "role": "system",
        "content": system_content
    }));

    if all_messages.is_empty() {
        return api_messages;
    }

    let first_msg = &all_messages[0];
    api_messages.push(format_message_for_llm(&first_msg.2, &first_msg.3));

    if all_messages.len() <= 1 {
        return api_messages;
    }

    let remaining = &all_messages[1..];

    let windowed = if remaining.len() <= window_size {
        remaining
    } else {
        let mut cut = remaining.len() - window_size;
        while cut > 0 && remaining[cut].2 != "user" {
            cut -= 1;
        }
        &remaining[cut..]
    };

    // Skip the mission start if it would be duplicated (window reaches back to index 0 of remaining,
    // which is index 1 of all_messages -- not a duplicate of index 0).
    for msg in windowed {
        let content = if msg.2 == "tool" {
            truncate_tool_content(&msg.3)
        } else {
            msg.3.clone()
        };
        api_messages.push(format_message_for_llm(&msg.2, &content));
    }

    api_messages
}

/// Find the largest byte index <= `max` that falls on a UTF-8 char boundary.
fn floor_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut i = max;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn format_message_for_llm(role: &str, content: &str) -> Value {
    if role == "tool" {
        json!({ "role": "user", "content": format!("[Tool result]\n{}", content) })
    } else {
        json!({ "role": role, "content": content })
    }
}
