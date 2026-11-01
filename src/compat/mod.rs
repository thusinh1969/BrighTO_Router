//! DeepSeek-cloud compatibility shim for local vLLM routes (repo file
//! `LLM_guide_needed.md`).
//!
//! The owner's DeepSeek agent harness speaks the **cloud** dialect
//! (<https://api-docs.deepseek.com/guides/thinking_mode>): `thinking`,
//! `reasoning_effort`, `reasoning.effort`, `output_config.effort`; it reads
//! `reasoning_content` and `usage.prompt_cache_{hit,miss}_tokens`; and it sends
//! assistant history back with both `reasoning` and `reasoning_content`.
//!
//! A local vLLM understands none of that natively: it takes reasoning switches
//! through `chat_template_kwargs`, returns `reasoning` only, rejects a message
//! that carries both reasoning keys (`duplicate field 'reasoning'`), and does not
//! report cache hit/miss. This module performs the same rewrites the Python
//! shim did, inside the router, and only for routes flagged `deepseek_compat`.

use serde_json::{Map, Value};

/// Cloud effort level -> vLLM `reasoning_effort` value.
fn map_effort(level: &str) -> &'static str {
    match level {
        "minimal" | "low" => "low",
        "medium" | "high" | "xhigh" => "high",
        "max" | "ultra" => "max",
        // Unknown/`none` are handled before this point; keep the cloud default.
        _ => "high",
    }
}

/// Extract the client's effort from the three accepted shapes, in precedence
/// order: top-level `reasoning_effort`, then `reasoning.effort`, then
/// `output_config.effort`.
fn take_effort(body: &mut Map<String, Value>) -> Option<Value> {
    let top = body.remove("reasoning_effort");
    let nested = body
        .remove("reasoning")
        .and_then(|v| v.get("effort").cloned());
    let anthropic = body
        .remove("output_config")
        .and_then(|v| v.get("effort").cloned());
    top.or(nested).or(anthropic)
}

/// Apply the DeepSeek-cloud -> vLLM request rewrite, in place.
///
/// * merges `thinking` / `reasoning_effort` into `chat_template_kwargs`
///   (never overwriting a key the client already set there);
/// * removes `thinking`, `reasoning_effort`, `reasoning`, `output_config` from
///   the forwarded body (vLLM ignores them at top level);
/// * collapses each message's `reasoning` + `reasoning_content` into one
///   `reasoning_content` (vLLM 400s on both).
pub fn convert_request(body: &mut Value) {
    let Some(obj) = body.as_object_mut() else {
        return;
    };

    // A client may send `chat_template_kwargs` already; merge, never overwrite.
    let mut kwargs: Map<String, Value> = obj
        .get("chat_template_kwargs")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // `thinking` is always consumed (dict or bare bool), even when malformed.
    let thinking = obj.remove("thinking");
    let effort = take_effort(obj);

    if !kwargs.contains_key("thinking") {
        match thinking {
            Some(Value::Object(t)) => {
                let enabled = t.get("type").and_then(|v| v.as_str()) != Some("disabled");
                kwargs.insert("thinking".to_string(), Value::Bool(enabled));
            }
            Some(Value::Bool(enabled)) => {
                kwargs.insert("thinking".to_string(), Value::Bool(enabled));
            }
            _ => {}
        }
    }

    if !kwargs.contains_key("reasoning_effort") {
        if let Some(effort) = effort {
            if effort.as_str() == Some("none") {
                // Cloud `none` means "no thinking at all", not "lowest effort".
                kwargs.insert("thinking".to_string(), Value::Bool(false));
            } else if let Some(n) = effort.as_i64() {
                kwargs.insert(
                    "reasoning_effort".to_string(),
                    Value::from(n.clamp(1, 100)),
                );
            } else {
                let level = effort.as_str().unwrap_or("high").to_ascii_lowercase();
                kwargs.insert(
                    "reasoning_effort".to_string(),
                    Value::String(map_effort(&level).to_string()),
                );
            }
        }
    }

    if !kwargs.is_empty() {
        obj.insert("chat_template_kwargs".to_string(), Value::Object(kwargs));
    }

    // Collapse the two reasoning keys the harness echoes back.
    if let Some(messages) = obj.get_mut("messages").and_then(|v| v.as_array_mut()) {
        for message in messages {
            let Some(msg) = message.as_object_mut() else {
                continue;
            };
            let has = msg.contains_key("reasoning_content") || msg.contains_key("reasoning");
            if !has {
                continue;
            }
            let rc = msg.remove("reasoning_content");
            let rs = msg.remove("reasoning");
            let value = rc.or(rs).filter(|v| !v.is_null());
            if let Some(value) = value {
                msg.insert("reasoning_content".to_string(), value);
            }
        }
    }
}

/// DeepSeek-cloud response fields, in place: `message`/`delta.reasoning_content`
/// next to vLLM's `reasoning`, and `usage.prompt_cache_{hit,miss}_tokens`.
pub fn fix_response(value: &mut Value) {
    if let Some(choices) = value.get_mut("choices").and_then(|v| v.as_array_mut()) {
        for choice in choices {
            let Some(choice) = choice.as_object_mut() else {
                continue;
            };
            for key in ["message", "delta"] {
                if let Some(msg) = choice.get_mut(key).and_then(|v| v.as_object_mut()) {
                    if msg.get("reasoning_content").is_none() {
                        if let Some(reasoning) = msg.get("reasoning").cloned() {
                            if !reasoning.is_null() {
                                msg.insert("reasoning_content".to_string(), reasoning);
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(usage) = value.get_mut("usage").and_then(|v| v.as_object_mut()) {
        if let Some(prompt_tokens) = usage.get("prompt_tokens").and_then(|v| v.as_i64()) {
            let hit = usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            usage
                .entry("prompt_cache_hit_tokens".to_string())
                .or_insert_with(|| Value::from(hit));
            usage
                .entry("prompt_cache_miss_tokens".to_string())
                .or_insert_with(|| Value::from(prompt_tokens - hit));
        }
    }
}

/// Rewrite one SSE line. Returns the bytes to forward (unchanged when the line
/// is not a JSON `data:` frame or does not parse).
pub fn fix_sse_line(line: &[u8]) -> Vec<u8> {
    let trimmed = line.strip_prefix(b"data:").map(|rest| {
        if rest.starts_with(b" ") {
            &rest[1..]
        } else {
            rest
        }
    });
    let Some(payload) = trimmed else {
        return line.to_vec();
    };
    if payload.windows(6).any(|w| w == b"[DONE]") {
        return line.to_vec();
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return line.to_vec();
    };
    fix_response(&mut value);
    match serde_json::to_vec(&value) {
        Ok(body) => {
            let mut out = Vec::with_capacity(body.len() + 6);
            out.extend_from_slice(b"data: ");
            out.extend_from_slice(&body);
            out
        }
        Err(_) => line.to_vec(),
    }
}

/// Fix a complete non-stream response body (no-op when it is not JSON).
pub fn fix_body(body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    fix_response(&mut value);
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

/// Incremental SSE fixer: chunks may split a line, so buffer the tail until the
/// terminating newline arrives.
#[derive(Default)]
pub struct SseFixer {
    buf: Vec<u8>,
}

impl SseFixer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::with_capacity(self.buf.len());
        let mut start = 0usize;
        while let Some(offset) = self.buf[start..].iter().position(|b| *b == b'\n') {
            let end = start + offset;
            out.extend_from_slice(&fix_sse_line(&self.buf[start..end]));
            out.push(b'\n');
            start = end + 1;
        }
        self.buf.drain(..start);
        out
    }

    pub fn finish(&mut self) -> Vec<u8> {
        let rest = std::mem::take(&mut self.buf);
        if rest.is_empty() {
            Vec::new()
        } else {
            fix_sse_line(&rest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn thinking_disabled_maps_to_false() {
        let mut body = json!({"thinking": {"type": "disabled"}, "messages": []});
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["thinking"], json!(false));
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn effort_levels_map_to_cloud_table() {
        for (level, expected) in [
            ("minimal", "low"),
            ("low", "low"),
            ("medium", "high"),
            ("high", "high"),
            ("xhigh", "high"),
            ("max", "max"),
            ("ultra", "max"),
        ] {
            let mut body = json!({"reasoning_effort": level, "messages": []});
            convert_request(&mut body);
            assert_eq!(
                body["chat_template_kwargs"]["reasoning_effort"],
                json!(expected),
                "level {level}"
            );
        }
    }

    #[test]
    fn effort_none_disables_thinking() {
        let mut body = json!({"reasoning_effort": "none", "messages": []});
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["thinking"], json!(false));
        assert!(body["chat_template_kwargs"].get("reasoning_effort").is_none());
    }

    #[test]
    fn effort_integer_is_clamped() {
        let mut body = json!({"reasoning_effort": 250, "messages": []});
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["reasoning_effort"], json!(100));
        let mut body = json!({"reasoning_effort": 0, "messages": []});
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["reasoning_effort"], json!(1));
    }

    #[test]
    fn nested_effort_shapes_are_read() {
        let mut body = json!({"reasoning": {"effort": "max"}, "messages": []});
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["reasoning_effort"], json!("max"));
        let mut body = json!({"output_config": {"effort": "low"}, "messages": []});
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["reasoning_effort"], json!("low"));
    }

    #[test]
    fn client_kwargs_are_not_overwritten() {
        let mut body = json!({
            "chat_template_kwargs": {"thinking": true, "reasoning_effort": "max"},
            "thinking": {"type": "disabled"},
            "reasoning_effort": "low",
            "messages": []
        });
        convert_request(&mut body);
        assert_eq!(body["chat_template_kwargs"]["thinking"], json!(true));
        assert_eq!(body["chat_template_kwargs"]["reasoning_effort"], json!("max"));
    }

    #[test]
    fn both_reasoning_keys_collapse_to_one() {
        let mut body = json!({"messages": [{"role": "assistant",
            "reasoning": "r1", "reasoning_content": "r2", "tool_calls": []}]});
        convert_request(&mut body);
        let msg = &body["messages"][0];
        assert_eq!(msg["reasoning_content"], json!("r2"));
        assert!(msg.get("reasoning").is_none());
    }

    #[test]
    fn reasoning_only_becomes_reasoning_content() {
        let mut body = json!({"messages": [{"role": "assistant", "reasoning": "r"}]});
        convert_request(&mut body);
        assert_eq!(body["messages"][0]["reasoning_content"], json!("r"));
    }

    #[test]
    fn null_reasoning_is_dropped() {
        let mut body = json!({"messages": [{"role": "assistant",
            "reasoning": null, "reasoning_content": null}]});
        convert_request(&mut body);
        assert!(body["messages"][0].get("reasoning_content").is_none());
        assert!(body["messages"][0].get("reasoning").is_none());
    }

    #[test]
    fn response_adds_reasoning_content_and_cache_usage() {
        let mut value = json!({
            "choices": [{"message": {"role": "assistant", "reasoning": "think", "content": "hi"}}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 5,
                      "prompt_tokens_details": {"cached_tokens": 40}}
        });
        fix_response(&mut value);
        assert_eq!(value["choices"][0]["message"]["reasoning_content"], json!("think"));
        assert_eq!(value["choices"][0]["message"]["reasoning"], json!("think"));
        assert_eq!(value["usage"]["prompt_cache_hit_tokens"], json!(40));
        assert_eq!(value["usage"]["prompt_cache_miss_tokens"], json!(60));
    }

    #[test]
    fn response_cache_usage_defaults_to_zero_hit() {
        let mut value = json!({"usage": {"prompt_tokens": 10}});
        fix_response(&mut value);
        assert_eq!(value["usage"]["prompt_cache_hit_tokens"], json!(0));
        assert_eq!(value["usage"]["prompt_cache_miss_tokens"], json!(10));
    }

    #[test]
    fn sse_delta_gets_reasoning_content() {
        let line = br#"data: {"choices":[{"delta":{"reasoning":"x"}}]}"#;
        let out = super::fix_sse_line(line);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("data: "));
        let value: Value = serde_json::from_str(&text[6..]).unwrap();
        assert_eq!(value["choices"][0]["delta"]["reasoning_content"], json!("x"));
    }

    #[test]
    fn sse_fixer_handles_split_lines() {
        let mut fixer = SseFixer::new();
        // A chunk boundary in the middle of a JSON frame.
        let first = br#"data: {"choices":[{"delta":{"re"#;
        let out = fixer.feed(first);
        assert!(out.is_empty(), "incomplete line must be buffered");
        let second = b"asoning\":\"x\"}}]}\n";
        let out = fixer.feed(second);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("data: "));
        assert!(text.ends_with('\n'));
        let value: Value = serde_json::from_str(text.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(value["choices"][0]["delta"]["reasoning_content"], json!("x"));
    }

    #[test]
    fn fix_body_adds_cloud_fields() {
        let body = br#"{"choices":[{"message":{"reasoning":"r"}}],"usage":{"prompt_tokens":5}}"#;
        let out = fix_body(body);
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["choices"][0]["message"]["reasoning_content"], json!("r"));
        assert_eq!(value["usage"]["prompt_cache_miss_tokens"], json!(5));
    }

    #[test]
    fn sse_done_is_untouched() {
        let line = b"data: [DONE]";
        assert_eq!(super::fix_sse_line(line), line.to_vec());
    }
}
