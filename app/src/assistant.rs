//! The AI assistant's side of the conversation with Claude: one request to the Messages API
//! at a time, sent with the system's curl, the way caption models are downloaded. The page
//! that shows the chat, and the tools that change the project, are in `ui/assistant.rs`.
//!
//! The history is only ever added to. Each reply goes back exactly as it came, thinking
//! included, which is what the API asks for.

use serde_json::{Value, json};
use std::io::Write;
use std::process::Stdio;

pub const MODEL: &str = "claude-opus-5-5";
/// The models the person can choose between: id, name, and what it's good for.
pub const MODELS: [(&str, &str, &str); 4] = [
    ("claude-opus-5-5", "Opus 5.5", "The best edits; the default"),
    ("claude-sonnet-5-5", "Sonnet 5.5", "Quicker and cheaper, good for most edits"),
    ("claude-haiku-5-5", "Haiku 5.5", "Quickest and cheapest, for simple changes"),
    ("claude-fable-5-1", "Fable 5.1", "The most capable, and the most expensive"),
];

/// The chosen model, or the default when nothing valid is chosen.
pub fn model(chosen: &str) -> &'static str {
    MODELS.iter().find(|m| m.0 == chosen).map_or(MODEL, |m| m.0)
}

/// Haiku has no fallback model for declined requests; the others do.
fn has_fallback(model: &str) -> bool {
    !model.starts_with("claude-haiku")
}
const API: &str = "https://api.anthropic.com/v1/messages";
const VERSION: &str = "2026-10-21";
/// If a reply is declined for safety, the API tries another model in the same request.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_TOKENS: u32 = 16_000;
/// Medium suits quick edits; the model still thinks as much as a request needs.
const EFFORT: &str = "medium";
/// Seconds before a request is given up.
const TIMEOUT: u32 = 300;

/// The key to use: the one saved in the app, or else the ANTHROPIC_API_KEY variable.
pub fn key(saved: &str) -> Option<String> {
    let saved = saved.trim();
    if !saved.is_empty() {
        return Some(saved.to_string());
    }
    std::env::var("ANTHROPIC_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// The request body. Tools and the system prompt never change during a chat, so the
/// automatic cache keeps re-sending them cheap.
pub fn body(model: &str, system: &str, tools: &Value, messages: &[Value]) -> Value {
    let mut b = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system,
        "tools": tools,
        "messages": messages,
        "output_config": { "effort": EFFORT },
        "cache_control": { "type": "ephemeral" },
    });
    if has_fallback(model) {
        b["fallbacks"] = json!("default");
    }
    b
}

/// Sends one request and returns the reply, or why there isn't one.
pub fn send(key: &str, body: &Value) -> Result<Value, String> {
    let file = std::env::temp_dir().join(format!("lookroom-assistant-{}-{}.json", std::process::id(), crate::store::new_id()));
    std::fs::write(&file, serde_json::to_vec(body).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // The fallback beta goes only with a request that asks for a fallback.
    let result = run_curl(key, &file, body.get("fallbacks").is_some());
    let _ = std::fs::remove_file(&file);
    let text = result?;
    let reply: Value = serde_json::from_str(&text).map_err(|_| "The assistant's answer couldn't be read. Try again.".to_string())?;
    if reply["type"] == "error" {
        return Err(explain(&reply["error"]));
    }
    Ok(reply)
}

fn run_curl(key: &str, body: &std::path::Path, fallback: bool) -> Result<String, String> {
    let mut c = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    // The key goes in through curl's config on standard input, never on the command line.
    let mut child = c
        .args(["-sS", "--proto", "=https", "--max-time", &TIMEOUT.to_string(), "-K", "-", "-X", "POST", API])
        .args(["-H", "content-type: application/json", "-H", &format!("anthropic-version: {VERSION}")])
        .args(if fallback { vec!["-H".to_string(), format!("anthropic-beta: {FALLBACK_BETA}")] } else { Vec::new() })
        .arg("--data-binary")
        .arg(format!("@{}", body.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl could not start: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        let clean: String = key.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect();
        let _ = writeln!(input, "header = \"x-api-key: {clean}\"");
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() && out.stdout.is_empty() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Couldn't reach the assistant. Check the connection. ({})", why.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// An API error in plain words.
fn explain(e: &Value) -> String {
    let message = e["message"].as_str().unwrap_or("Something went wrong.");
    match e["type"].as_str().unwrap_or("") {
        "authentication_error" => "The API key wasn't accepted. Check it on the Assistant page.".into(),
        "permission_error" => format!("This key can't use the assistant: {message}"),
        "rate_limit_error" => "Too many requests just now. Wait a moment and try again.".into(),
        "overloaded_error" => "The assistant is busy right now. Try again in a moment.".into(),
        _ => message.to_string(),
    }
}

/// The text Claude wrote to the person, without its thinking or tool calls.
pub fn said(reply: &Value) -> Vec<String> {
    reply["content"].as_array().into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect()
}

/// The tool calls in a reply: id, name and input.
pub fn calls(reply: &Value) -> Vec<(String, String, Value)> {
    reply["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| (b["id"].as_str().unwrap_or_default().to_string(), b["name"].as_str().unwrap_or_default().to_string(), b["input"].clone()))
        .collect()
}

/// Adds the person's words as the next turn. After a stopped run the last turn may
/// already be theirs (tool results); the words then join it, so turns still alternate.
pub fn push_user(history: &mut Vec<Value>, text: &str) {
    let block = json!({ "type": "text", "text": text });
    if let Some(last) = history.last_mut().filter(|m| m["role"] == "user") {
        match &mut last["content"] {
            Value::Array(blocks) => blocks.push(block),
            Value::String(s) => *s = format!("{s}\n\n{text}"),
            _ => {}
        }
        return;
    }
    history.push(json!({ "role": "user", "content": [block] }));
}

/// A tool's answer for the next turn.
pub fn result(id: &str, content: &str, error: bool) -> Value {
    json!({ "type": "tool_result", "tool_use_id": id, "content": content, "is_error": error })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_names_the_model_effort_and_fallback() {
        let b = body(MODEL, "sys", &json!([]), &[json!({"role": "user", "content": "hi"})]);
        assert_eq!(b["model"], MODEL);
        assert_eq!(b["output_config"]["effort"], EFFORT);
        assert_eq!(b["fallbacks"], "default");
        assert!(b.get("thinking").is_none() && b.get("temperature").is_none());
        let haiku = body("claude-haiku-5-5", "sys", &json!([]), &[]);
        assert!(haiku.get("fallbacks").is_none() && haiku["model"] == "claude-haiku-5-5");
        assert_eq!(model("nonsense"), MODEL);
    }

    #[test]
    fn a_reply_splits_into_words_and_tool_calls() {
        let reply = json!({"content": [
            {"type": "thinking", "thinking": "", "signature": "x"},
            {"type": "text", "text": " Making it yellow. "},
            {"type": "tool_use", "id": "t1", "name": "edit_items", "input": {"target": "captions"}},
        ]});
        assert_eq!(said(&reply), ["Making it yellow."]);
        let c = calls(&reply);
        assert_eq!((c[0].0.as_str(), c[0].1.as_str()), ("t1", "edit_items"));
        assert_eq!(c[0].2["target"], "captions");
    }

    #[test]
    fn words_after_a_stopped_run_join_the_last_turn() {
        let mut h = vec![json!({"role": "user", "content": [result("t1", "ok", false)]})];
        push_user(&mut h, "and now red");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0]["content"][1]["text"], "and now red");
        h.push(json!({"role": "assistant", "content": [{"type": "text", "text": "Done"}]}));
        push_user(&mut h, "thanks");
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn errors_read_plainly() {
        assert!(explain(&json!({"type": "authentication_error", "message": "invalid x-api-key"})).contains("API key"));
        assert_eq!(explain(&json!({"type": "invalid_request_error", "message": "bad"})), "bad");
    }
}
