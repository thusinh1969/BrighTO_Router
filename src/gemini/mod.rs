//! Google Gemini adapter for the OpenAI-compatible endpoint
//! (`https://generativelanguage.googleapis.com/v1beta/openai`).
//!
//! Gemini speaks Chat Completions but validates the payload strictly: any field
//! Google does not implement is a hard 400
//! `Invalid JSON payload received. Unknown name "store": Cannot find field.`
//! — and a JSON `null` still counts as "present", so the field has to be
//! **removed**, not nulled.
//!
//! Probed against `gemini-4-hydrogen` (2026-11-14). Rejected fields:
//! `store`, `seed`, `logprobs`, `top_logprobs`, `frequency_penalty`,
//! `logit_bias`, `metadata`, `prediction`, `web_search_options`.
//! Accepted (leave untouched): `n`, `presence_penalty`, `parallel_tool_calls`,
//! `tools`, `tool_choice`, `stream`, `stream_options`, `response_format`,
//! `service_tier`, `user`, `stop`, `modalities`, `reasoning_effort`,
//! `max_tokens` / `max_completion_tokens` (one of them, not both).
//!
//! This module owns that knowledge so routes only declare `protocol:
//! gemini_chat`; no per-route field list is needed.

use serde_json::Value;

/// OpenAI request fields Google's endpoint rejects outright.
pub const UNSUPPORTED_FIELDS: &[&str] = &[
    "store",
    "seed",
    "logprobs",
    "top_logprobs",
    "frequency_penalty",
    "logit_bias",
    "metadata",
    "prediction",
    "web_search_options",
];

/// Remove Gemini-incompatible fields in place. Returns true when something changed.
pub fn normalize_request(value: &mut Value) -> bool {
    let Some(obj) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    for field in UNSUPPORTED_FIELDS {
        if obj.remove(*field).is_some() {
            changed = true;
        }
    }
    // Gemini rejects `max_tokens` together with `max_completion_tokens`; keep the
    // OpenAI-preferred name when both are present.
    if obj.contains_key("max_tokens") && obj.contains_key("max_completion_tokens") {
        obj.remove("max_tokens");
        changed = true;
    }
    changed
}

/// Normalize a whole JSON request body. `None` when the body is not JSON or
/// nothing needed removing (the caller then forwards the original bytes).
pub fn normalize_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body).ok()?;
    if !normalize_request(&mut value) {
        return None;
    }
    serde_json::to_vec(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_every_rejected_field() {
        let mut body = json!({
            "model": "gemini-4-hydrogen",
            "messages": [],
            "store": false,
            "seed": 1,
            "logprobs": true,
            "top_logprobs": 1,
            "frequency_penalty": 0.0,
            "logit_bias": {},
            "metadata": {"a": "b"},
            "prediction": null,
            "web_search_options": {},
        });
        assert!(normalize_request(&mut body));
        let obj = body.as_object().unwrap();
        for field in UNSUPPORTED_FIELDS {
            assert!(!obj.contains_key(*field), "{field} should be removed");
        }
        assert_eq!(body["model"], json!("gemini-4-hydrogen"));
        assert_eq!(body["messages"], json!([]));
    }

    #[test]
    fn keeps_accepted_fields() {
        let mut body = json!({
            "model": "m", "messages": [],
            "n": 1, "presence_penalty": 0.0, "parallel_tool_calls": true,
            "tools": [], "tool_choice": "auto", "stream": true,
            "stream_options": {"include_usage": true}, "response_format": {"type": "text"},
            "service_tier": "auto", "user": "u", "stop": ["x"],
            "modalities": ["text"], "reasoning_effort": "low", "max_tokens": 8,
        });
        assert!(!normalize_request(&mut body));
        let obj = body.as_object().unwrap();
        for field in ["n", "presence_penalty", "parallel_tool_calls", "stream", "max_tokens"] {
            assert!(obj.contains_key(field), "{field} must survive");
        }
    }

    #[test]
    fn drops_max_tokens_when_max_completion_tokens_present() {
        let mut body = json!({"messages": [], "max_tokens": 8, "max_completion_tokens": 16});
        assert!(normalize_request(&mut body));
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["max_completion_tokens"], json!(16));
    }

    #[test]
    fn null_value_is_still_stripped() {
        let mut body = json!({"messages": [], "store": null, "prediction": null});
        assert!(normalize_request(&mut body));
        assert!(body.get("store").is_none());
        assert!(body.get("prediction").is_none());
    }

    #[test]
    fn non_json_body_is_left_alone() {
        assert!(normalize_body(b"not json").is_none());
        assert!(normalize_body(br#"{"model":"m","messages":[]}"#).is_none());
        assert!(normalize_body(br#"{"model":"m","store":false}"#).is_some());
    }
}
