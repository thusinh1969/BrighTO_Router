//! OpenAI Chat Completions non-stream response -> Anthropic Messages response.

use serde_json::{Map, Value, json};

use super::{BridgeContext, BridgeError, bridge_message_id, thinking_signature};

/// Translate one complete OpenAI Chat Completion body into an Anthropic
/// `message` object. The returned value is also used as the source for the
/// synthetic SSE sequence when an upstream ignores `stream`.
pub fn build_message(body: &[u8], ctx: &BridgeContext) -> Result<Value, BridgeError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| BridgeError::Upstream(format!("invalid upstream JSON: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| BridgeError::Upstream("upstream response must be a JSON object".into()))?;
    if let Some(error) = object.get("error") {
        return Err(BridgeError::Upstream(error_message(error)));
    }
    let choice = object
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object)
        .ok_or_else(|| BridgeError::Upstream("upstream response has no choices".into()))?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| BridgeError::Upstream("upstream choice has no message".into()))?;

    let mut content = Vec::new();
    if let Some(reasoning) = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .or_else(|| message.get("thinking"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        content.push(json!({
            "type": "thinking",
            "thinking": reasoning,
            "signature": thinking_signature(reasoning),
        }));
    }

    let mut text_blocks = extract_text_blocks(message.get("content"))?;
    if text_blocks.is_empty()
        && let Some(refusal) = message.get("refusal").and_then(Value::as_str)
        && !refusal.is_empty()
    {
        text_blocks.push(refusal.to_string());
    }
    let mut tool_calls = extract_tool_calls(message, &ctx.request_id)?;
    for text in text_blocks {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }

    let finish = choice.get("finish_reason").and_then(Value::as_str);
    let has_tools = !tool_calls.is_empty();
    content.append(&mut tool_calls);
    if content.is_empty() {
        content.push(json!({"type": "text", "text": ""}));
    }

    let (prompt_tokens, completion_tokens) = usage_from(&value);
    let input_tokens = if prompt_tokens > 0 {
        prompt_tokens
    } else {
        ctx.estimated_input_tokens
    };

    Ok(json!({
        "id": bridge_message_id(&ctx.request_id, object.get("id").and_then(Value::as_str)),
        "type": "message",
        "role": "assistant",
        "model": ctx.public_model,
        "content": content,
        "stop_reason": stop_reason(finish, has_tools),
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": completion_tokens,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
        },
    }))
}

pub fn openai_to_anthropic(body: &[u8], ctx: &BridgeContext) -> Result<Vec<u8>, BridgeError> {
    let value = build_message(body, ctx)?;
    serde_json::to_vec(&value)
        .map_err(|error| BridgeError::Upstream(format!("could not encode response: {error}")))
}

pub(crate) fn stop_reason(finish: Option<&str>, has_tools: bool) -> &'static str {
    match finish {
        Some("length") | Some("max_tokens") => "max_tokens",
        Some("tool_calls") | Some("function_call") => {
            if has_tools {
                "tool_use"
            } else {
                "end_turn"
            }
        }
        Some("content_filter") => "end_turn",
        Some("stop") | Some("end_turn") | Some("eos") | Some("") | None => {
            if has_tools {
                "tool_use"
            } else {
                "end_turn"
            }
        }
        // Unknown provider-specific reasons are closest to a normal end.
        Some(_) => {
            if has_tools {
                "tool_use"
            } else {
                "end_turn"
            }
        }
    }
}

/// Read prompt/completion tokens from an OpenAI `usage` object. Handles both
/// the Chat Completions and the Responses/input-output naming used by some
/// OpenAI-compatible providers.
pub(crate) fn usage_from(value: &Value) -> (u64, u64) {
    let usage = value.get("usage").unwrap_or(value);
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (input, output)
}

fn extract_text_blocks(content: Option<&Value>) -> Result<Vec<String>, BridgeError> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };
    if content.is_null() {
        return Ok(Vec::new());
    }
    if let Some(text) = content.as_str() {
        return Ok(vec![text.to_string()]);
    }
    let parts = content
        .as_array()
        .ok_or_else(|| BridgeError::Upstream("upstream message content is not text".into()))?;
    let mut out = Vec::new();
    for part in parts {
        let Some(object) = part.as_object() else {
            continue;
        };
        let kind = object.get("type").and_then(Value::as_str).unwrap_or("text");
        if matches!(kind, "text" | "output_text" | "refusal")
            && let Some(text) = object.get("text").and_then(Value::as_str)
        {
            out.push(text.to_string());
        }
    }
    Ok(out)
}

fn extract_tool_calls(
    message: &Map<String, Value>,
    request_id: &str,
) -> Result<Vec<Value>, BridgeError> {
    let mut out = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let object = call.as_object().ok_or_else(|| {
                BridgeError::Upstream("upstream tool call must be an object".into())
            })?;
            out.push(tool_block(object, index, request_id)?);
        }
    }
    if out.is_empty()
        && let Some(call) = message.get("function_call").and_then(Value::as_object)
    {
        let mut object = Map::new();
        object.insert("function".into(), Value::Object(call.clone()));
        out.push(tool_block(&object, 0, request_id)?);
    }
    Ok(out)
}

fn tool_block(
    call: &Map<String, Value>,
    index: usize,
    request_id: &str,
) -> Result<Value, BridgeError> {
    let function = call
        .get("function")
        .and_then(Value::as_object)
        .ok_or_else(|| BridgeError::Upstream("upstream tool call has no function".into()))?;
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BridgeError::Upstream("upstream tool call has no name".into()))?;
    let id = call
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| synthetic_tool_id(request_id, index));
    let input = tool_input(function.get("arguments"))?;
    Ok(json!({
        "type": "tool_use",
        "id": id,
        "name": name,
        "input": input,
    }))
}

pub(crate) fn tool_input(arguments: Option<&Value>) -> Result<Value, BridgeError> {
    let raw = match arguments {
        None | Some(Value::Null) => "{}".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(_)) => arguments
            .cloned()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "{}".to_string()),
        Some(_) => {
            return Err(BridgeError::Upstream(
                "upstream tool arguments must be a JSON string or object".into(),
            ));
        }
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(raw).map_err(|error| {
        BridgeError::Upstream(format!(
            "upstream tool arguments are not valid JSON: {error}"
        ))
    })?;
    value
        .as_object()
        .map(|object| Value::Object(object.clone()))
        .ok_or_else(|| {
            BridgeError::Upstream("upstream tool arguments must be a JSON object".into())
        })
}

pub(crate) fn synthetic_tool_id(request_id: &str, index: usize) -> String {
    let safe: String = request_id
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if safe.is_empty() {
        format!("toolu_bridge_{index}")
    } else {
        format!("toolu_bridge_{safe}_{index}")
    }
}

fn error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .unwrap_or("upstream returned an error")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> BridgeContext {
        BridgeContext {
            public_model: "public-model".into(),
            request_id: "req-1".into(),
            estimated_input_tokens: 42,
        }
    }

    fn translate(value: Value) -> Value {
        build_message(serde_json::to_string(&value).unwrap().as_bytes(), &ctx())
            .expect("translation")
    }

    #[test]
    fn maps_text_usage_and_stop_reason() {
        let out = translate(json!({
            "id": "chatcmpl-1",
            "choices": [{
                "message": {"role": "assistant", "content": "hello"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3}
        }));
        assert_eq!(out["id"], "chatcmpl-1");
        assert_eq!(out["type"], "message");
        assert_eq!(out["model"], "public-model");
        assert_eq!(out["content"][0]["type"], "text");
        assert_eq!(out["content"][0]["text"], "hello");
        assert_eq!(out["stop_reason"], "end_turn");
        assert_eq!(out["usage"]["input_tokens"], 12);
        assert_eq!(out["usage"]["output_tokens"], 3);
    }

    #[test]
    fn maps_tool_calls_and_arguments() {
        let out = translate(json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {"id": "call_1", "type": "function", "function": {"name": "Read", "arguments": "{\"path\":\"a.txt\"}"}},
                        {"type": "function", "function": {"name": "Glob", "arguments": "{\"pattern\":\"*.rs\"}"}}
                    ]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"input_tokens": 7, "output_tokens": 9}
        }));
        assert_eq!(out["stop_reason"], "tool_use");
        assert_eq!(out["content"][0]["type"], "tool_use");
        assert_eq!(out["content"][0]["input"]["path"], "a.txt");
        assert_eq!(out["content"][1]["type"], "tool_use");
        assert!(
            out["content"][1]["id"]
                .as_str()
                .unwrap()
                .starts_with("toolu_bridge_")
        );
        assert_eq!(out["usage"]["input_tokens"], 7);
    }

    #[test]
    fn maps_reasoning_before_text() {
        let out = translate(json!({
            "choices": [{
                "message": {"role": "assistant", "content": "answer", "reasoning_content": "why"},
                "finish_reason": "stop"
            }]
        }));
        assert_eq!(out["content"][0]["type"], "thinking");
        assert_eq!(out["content"][0]["thinking"], "why");
        assert_eq!(out["content"][1]["type"], "text");
        assert_eq!(out["usage"]["input_tokens"], 42, "falls back to estimate");
    }

    #[test]
    fn maps_content_parts_and_length() {
        let out = translate(json!({
            "choices": [{
                "message": {"role": "assistant", "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]},
                "finish_reason": "length"
            }]
        }));
        assert_eq!(out["content"][0]["text"], "a");
        assert_eq!(out["content"][1]["text"], "b");
        assert_eq!(out["stop_reason"], "max_tokens");
    }

    #[test]
    fn rejects_upstream_errors_and_invalid_arguments() {
        let err = build_message(br#"{"error":{"message":"bad key"}}"#, &ctx()).unwrap_err();
        assert!(matches!(err, BridgeError::Upstream(message) if message == "bad key"));

        let err = build_message(
            br#"{"choices":[{"message":{"tool_calls":[{"function":{"name":"Read","arguments":"not json"}}]},"finish_reason":"tool_calls"}]}"#,
            &ctx(),
        )
        .unwrap_err();
        assert!(matches!(err, BridgeError::Upstream(_)));
    }
}
