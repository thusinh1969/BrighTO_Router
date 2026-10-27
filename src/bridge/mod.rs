//! Protocol bridges for LLM chat traffic.
//!
//! The only supported bridge is Anthropic Messages (Claude CLI) -> OpenAI Chat
//! Completions. Normal OpenAI/Anthropic routes remain byte pass-through; this
//! module is called only when a route has `accept_anthropic_messages` enabled.

pub mod request;
pub mod response;
pub mod stream;
pub mod tokens;

use bytes::Bytes;
use futures::StreamExt;
use serde_json::Value;

/// Non-stream bridge responses are fully buffered; this is a hard ceiling.
pub const NONSTREAM_RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
/// Upstream error bodies are only used to extract a human-readable message.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;
/// An SSE line larger than this is treated as a malformed upstream stream.
pub const SSE_LINE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BridgeContext {
    pub public_model: String,
    pub request_id: String,
    pub estimated_input_tokens: u64,
}

#[derive(Debug, Clone)]
pub enum BridgeError {
    /// The client request cannot be translated into a valid OpenAI request.
    InvalidRequest(String),
    /// A valid Anthropic feature has no safe OpenAI equivalent.
    Unsupported(String),
    /// The upstream returned or streamed data we cannot translate.
    Upstream(String),
    /// A response exceeded a bridge buffer limit.
    TooLarge,
}

impl BridgeError {
    pub fn status(&self) -> u16 {
        match self {
            Self::InvalidRequest(_) | Self::Unsupported(_) => 400,
            Self::Upstream(_) | Self::TooLarge => 502,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::InvalidRequest(message)
            | Self::Unsupported(message)
            | Self::Upstream(message) => message.clone(),
            Self::TooLarge => "bridge response exceeded the size limit".to_string(),
        }
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for BridgeError {}

/// Build one Anthropic SSE frame. Anthropic clients read the `event:` name and
/// the JSON body; both must always be present.
pub fn sse_event(event: &str, data: &Value) -> Vec<u8> {
    let body = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    let mut out = Vec::with_capacity(event.len() + body.len() + 16);
    out.extend_from_slice(b"event: ");
    out.extend_from_slice(event.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(b"data: ");
    out.extend_from_slice(body.as_bytes());
    out.extend_from_slice(b"\n\n");
    out
}

pub fn anthropic_sse_error(status: u16, message: &str) -> Vec<u8> {
    let body = anthropic_error_body(status, message);
    let value: Value =
        serde_json::from_slice(&body).unwrap_or_else(|_| serde_json::json!({"type": "error"}));
    sse_event("error", &value)
}

/// Anthropic thinking blocks require a signature. We have no upstream signature
/// when bridging; this opaque, deterministic value lets the client round-trip
/// history while the request side drops unsigned thinking blocks safely.
pub fn thinking_signature(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("brighto-bridge-{hash:016x}")
}

pub fn bridge_message_id(request_id: &str, upstream: Option<&str>) -> String {
    if let Some(id) = upstream.filter(|value| !value.is_empty()) {
        return id.to_string();
    }
    let safe: String = request_id
        .chars()
        .filter(|value| value.is_ascii_alphanumeric() || *value == '-' || *value == '_')
        .take(48)
        .collect();
    if safe.is_empty() {
        "msg_bridge".to_string()
    } else {
        format!("msg_bridge_{safe}")
    }
}

pub fn anthropic_error_type(status: u16) -> &'static str {
    match status {
        400 | 422 => "invalid_request_error",
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        413 => "request_too_large",
        429 => "rate_limit_error",
        503 | 529 => "overloaded_error",
        500..=599 => "api_error",
        _ => "api_error",
    }
}

pub fn anthropic_error_body(status: u16, message: &str) -> Vec<u8> {
    let message = truncate_message(message);
    serde_json::to_vec(&serde_json::json!({
        "type": "error",
        "error": {
            "type": anthropic_error_type(status),
            "message": message,
        }
    }))
    .unwrap_or_else(|_| {
        b"{\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"bridge error\"}}"
            .to_vec()
    })
}

/// Translate an upstream OpenAI error (or an already-Anthropic error) into the
/// Anthropic error envelope. Provider error bodies are not persisted.
pub fn translate_upstream_error(status: u16, body: &[u8], fallback: &str) -> Vec<u8> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if value.get("type").and_then(Value::as_str) == Some("error")
            && value.get("error").is_some()
        {
            return body.to_vec();
        }
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
            .unwrap_or(fallback);
        return anthropic_error_body(status, message);
    }
    anthropic_error_body(status, fallback)
}

pub fn is_anthropic_error_body(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .is_some_and(|value| {
            value.get("type").and_then(Value::as_str) == Some("error")
                && value.get("error").is_some()
        })
}

pub async fn read_body_limited(
    response: reqwest::Response,
    limit: usize,
) -> Result<Bytes, BridgeError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(BridgeError::TooLarge);
    }
    let mut out = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| BridgeError::Upstream(error.to_string()))?;
        if out.len().saturating_add(chunk.len()) > limit {
            return Err(BridgeError::TooLarge);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(out))
}

fn truncate_message(message: &str) -> String {
    const MAX: usize = 2000;
    if message.chars().count() <= MAX {
        return message.to_string();
    }
    let mut out: String = message.chars().take(MAX).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_envelopes_map_statuses() {
        assert!(is_anthropic_error_body(&anthropic_error_body(
            401,
            "invalid key"
        )));
        let body = translate_upstream_error(
            429,
            br#"{"error":{"message":"too many requests","type":"rate_limit"}}"#,
            "fallback",
        );
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["type"], "rate_limit_error");
        assert_eq!(value["error"]["message"], "too many requests");
    }

    #[test]
    fn already_anthropic_error_passes_through() {
        let body = anthropic_error_body(400, "bad request");
        assert_eq!(
            translate_upstream_error(500, &body, "fallback"),
            body,
            "an Anthropic error body must not be double-wrapped"
        );
    }
}
