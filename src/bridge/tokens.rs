//! Local input-token estimation for `/v1/messages/count_tokens` and for the
//! `message_start` usage estimate. This is deliberately approximate: the
//! upstream provider reports the authoritative usage in the normal response.

use serde_json::Value;

use super::BridgeError;

const DEFAULT_CHARS_PER_TOKEN: f64 = 3.5;
const IMAGE_TOKENS: u64 = 1000;
const MESSAGE_OVERHEAD: u64 = 4;

pub fn estimate_input_tokens(body: &[u8], chars_per_token: f64) -> Result<u64, BridgeError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| BridgeError::InvalidRequest(format!("invalid JSON: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| BridgeError::InvalidRequest("request body must be a JSON object".into()))?;
    let mut counter = Counter::default();

    if let Some(system) = object.get("system") {
        count_system(system, &mut counter);
    }
    if let Some(messages) = object.get("messages").and_then(Value::as_array) {
        counter.overhead = counter
            .overhead
            .saturating_add((messages.len() as u64).saturating_mul(MESSAGE_OVERHEAD));
        for message in messages {
            if let Some(content) = message.get("content") {
                count_content(content, &mut counter);
            }
        }
    }
    if let Some(tools) = object.get("tools").and_then(Value::as_array) {
        for tool in tools {
            if let Some(name) = tool.get("name").and_then(Value::as_str) {
                counter.chars = counter.chars.saturating_add(name.chars().count() as u64);
            }
            if let Some(description) = tool.get("description").and_then(Value::as_str) {
                counter.chars = counter
                    .chars
                    .saturating_add(description.chars().count() as u64);
            }
            if let Some(schema) = tool.get("input_schema")
                && let Ok(encoded) = serde_json::to_string(schema)
            {
                counter.chars = counter.chars.saturating_add(encoded.chars().count() as u64);
            }
        }
    }

    let divisor = if chars_per_token.is_finite() && chars_per_token >= 1.0 {
        chars_per_token
    } else {
        DEFAULT_CHARS_PER_TOKEN
    };
    let text_tokens = (counter.chars as f64 / divisor).ceil() as u64;
    let total = text_tokens
        .saturating_add(counter.extra_tokens)
        .saturating_add(counter.overhead);
    Ok(total.max(1))
}

#[derive(Default)]
struct Counter {
    chars: u64,
    extra_tokens: u64,
    overhead: u64,
}

fn count_system(system: &Value, counter: &mut Counter) {
    if let Some(text) = system.as_str() {
        counter.chars = counter.chars.saturating_add(text.chars().count() as u64);
        return;
    }
    if let Some(blocks) = system.as_array() {
        for block in blocks {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                counter.chars = counter.chars.saturating_add(text.chars().count() as u64);
            }
        }
    }
}

fn count_content(content: &Value, counter: &mut Counter) {
    if let Some(text) = content.as_str() {
        counter.chars = counter.chars.saturating_add(text.chars().count() as u64);
        return;
    }
    let Some(blocks) = content.as_array() else {
        return;
    };
    for block in blocks {
        let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "text" | "thinking" => {
                if let Some(text) = block
                    .get("text")
                    .or_else(|| block.get("thinking"))
                    .and_then(Value::as_str)
                {
                    counter.chars = counter.chars.saturating_add(text.chars().count() as u64);
                }
            }
            "tool_use" => {
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    counter.chars = counter.chars.saturating_add(name.chars().count() as u64);
                }
                if let Some(input) = block.get("input")
                    && let Ok(encoded) = serde_json::to_string(input)
                {
                    counter.chars = counter.chars.saturating_add(encoded.chars().count() as u64);
                }
            }
            "tool_result" => {
                if let Some(content) = block.get("content") {
                    count_content(content, counter);
                }
            }
            "image" => {
                counter.extra_tokens = counter.extra_tokens.saturating_add(IMAGE_TOKENS);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_text_and_scales_with_length() {
        let short =
            estimate_input_tokens(br#"{"messages":[{"role":"user","content":"hello"}]}"#, 3.5)
                .unwrap();
        let long = estimate_input_tokens(
            br#"{"messages":[{"role":"user","content":"hello world, this is a longer message with tools"}]}"#,
            3.5,
        )
        .unwrap();
        assert!(short >= 1);
        assert!(long > short);
    }

    #[test]
    fn counts_system_tools_and_images() {
        let text_only = estimate_input_tokens(
            br#"{"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
            3.5,
        )
        .unwrap();
        let with_image = estimate_input_tokens(
            br#"{"messages":[{"role":"user","content":[{"type":"text","text":"hi"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}]}"#,
            3.5,
        )
        .unwrap();
        assert!(with_image >= text_only + IMAGE_TOKENS);

        let with_tool = estimate_input_tokens(
            br#"{"system":"be concise","tools":[{"name":"Read","description":"read","input_schema":{"type":"object"}}],"messages":[{"role":"user","content":"hi"}]}"#,
            3.5,
        )
        .unwrap();
        assert!(with_tool > text_only);
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(
            estimate_input_tokens(b"not json", 3.5),
            Err(BridgeError::InvalidRequest(_))
        ));
    }
}
