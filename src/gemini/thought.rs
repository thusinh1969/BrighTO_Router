//! Gemini thought-signature carry-over.
//!
//! With tools enabled, Gemini binds every returned `functionCall` to an opaque
//! `thought_signature` and **requires it back** on the next turn for the most
//! recent tool call:
//!
//! ```text
//! 400 Function call is missing a thought_signature in functionCall parts.
//! ```
//!
//! (Verified: a tool call that is the last assistant message without the
//! signature is rejected; the same call buried under a later assistant answer is
//! accepted. Both streaming and non-streaming responses carry the signature in
//! `tool_calls[].extra_content.google.thought_signature`.)
//!
//! Generic OpenAI clients rebuild `tool_calls` from id/name/arguments and drop
//! `extra_content`, so the signature is lost. The router therefore keeps a small
//! bounded cache `tool_call_id -> signature`, records it when a Gemini response
//! goes by, and re-injects it into the last assistant tool call of the next
//! request.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// Cap the cache so a long-running router cannot grow without bound. Entries are
/// short-lived by nature (one tool turn); overflow simply clears the map.
const MAX_ENTRIES: usize = 4096;

fn cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remember the signature of one tool call.
pub fn record(id: &str, signature: &str) {
    if id.is_empty() || signature.is_empty() {
        return;
    }
    if let Ok(mut map) = cache().lock() {
        if map.len() >= MAX_ENTRIES {
            map.clear();
        }
        map.insert(id.to_string(), signature.to_string());
    }
}

/// Signature previously seen for this tool call id.
pub fn lookup(id: &str) -> Option<String> {
    cache().lock().ok()?.get(id).cloned()
}

fn signature_of(tool_call: &Value) -> Option<&str> {
    tool_call
        .get("extra_content")
        .and_then(|v| v.get("google"))
        .and_then(|v| v.get("thought_signature"))
        .and_then(|v| v.as_str())
}

/// Walk a Gemini response (non-stream `message` and streamed `delta` choices) and
/// cache every thought signature it carries.
pub fn record_from_response(value: &Value) {
    let Some(choices) = value.get("choices").and_then(|v| v.as_array()) else {
        return;
    };
    for choice in choices {
        for key in ["message", "delta"] {
            let Some(calls) = choice
                .get(key)
                .and_then(|m| m.get("tool_calls"))
                .and_then(|v| v.as_array())
            else {
                continue;
            };
            for call in calls {
                let (Some(id), Some(sig)) =
                    (call.get("id").and_then(|v| v.as_str()), signature_of(call))
                else {
                    continue;
                };
                // A streamed tool call may arrive in pieces (id in the first chunk,
                // extra_content later or vice versa); only record complete pairs.
                if !id.is_empty() {
                    record(id, sig);
                }
            }
        }
    }
}

/// Re-inject cached signatures into the **last** assistant tool call of a request.
/// Returns true when something changed.
pub fn inject_into_request(value: &mut Value) -> bool {
    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let Some(last) = messages
        .iter_mut()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
    else {
        return false;
    };
    let Some(calls) = last.get_mut("tool_calls").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let mut changed = false;
    for call in calls {
        let Some(obj) = call.as_object_mut() else {
            continue;
        };
        let signed = obj
            .get("extra_content")
            .and_then(|v| v.get("google"))
            .and_then(|v| v.get("thought_signature"))
            .is_some();
        if signed {
            continue;
        }
        let Some(id) = obj.get("id").and_then(|v| v.as_str()).map(str::to_string) else {
            continue;
        };
        let Some(sig) = lookup(&id) else {
            continue;
        };
        obj.insert(
            "extra_content".to_string(),
            serde_json::json!({"google": {"thought_signature": sig}}),
        );
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_and_injects_for_last_tool_call() {
        let resp = json!({"choices": [{"message": {"tool_calls": [
            {"id": "call_a", "type": "function", "function": {"name": "f", "arguments": "{}"},
             "extra_content": {"google": {"thought_signature": "SIG_A"}}}
        ]}}]});
        record_from_response(&resp);
        assert_eq!(lookup("call_a").as_deref(), Some("SIG_A"));

        let mut req = json!({
            "messages": [
                {"role": "user", "content": "call it"},
                {"role": "assistant", "content": null,
                 "tool_calls": [{"id": "call_a", "type": "function",
                                 "function": {"name": "f", "arguments": "{}"}}]},
                {"role": "tool", "tool_call_id": "call_a", "content": "{}"}
            ]
        });
        assert!(inject_into_request(&mut req));
        let tc = &req["messages"][1]["tool_calls"][0];
        assert_eq!(
            tc["extra_content"]["google"]["thought_signature"],
            json!("SIG_A")
        );
    }

    #[test]
    fn leaves_unsigned_unknown_calls_alone() {
        let mut req = json!({"messages": [
            {"role": "assistant", "content": null,
             "tool_calls": [{"id": "call_unknown", "type": "function",
                             "function": {"name": "f", "arguments": "{}"}}]}
        ]});
        assert!(!inject_into_request(&mut req));
        assert!(
            req["messages"][0]["tool_calls"][0]
                .get("extra_content")
                .is_none()
        );
    }

    #[test]
    fn only_the_last_assistant_call_is_touched() {
        let resp = json!({"choices": [{"message": {"tool_calls": [
            {"id": "call_old", "extra_content": {"google": {"thought_signature": "OLD"}}},
            {"id": "call_new", "extra_content": {"google": {"thought_signature": "NEW"}}}
        ]}}]});
        record_from_response(&resp);
        let mut req = json!({"messages": [
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_old", "type": "function", "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "call_old", "content": "{}"},
            {"role": "assistant", "content": "done"},
            {"role": "user", "content": "next"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_new", "type": "function", "function": {"name": "f", "arguments": "{}"}}]}
        ]});
        assert!(inject_into_request(&mut req));
        assert!(
            req["messages"][0]["tool_calls"][0]
                .get("extra_content")
                .is_none()
        );
        assert_eq!(
            req["messages"][4]["tool_calls"][0]["extra_content"]["google"]["thought_signature"],
            json!("NEW")
        );
    }

    #[test]
    fn stream_deltas_are_recorded() {
        let chunk = json!({"choices": [{"delta": {"tool_calls": [
            {"id": "call_s", "extra_content": {"google": {"thought_signature": "S"}}}
        ]}}]});
        record_from_response(&chunk);
        assert_eq!(lookup("call_s").as_deref(), Some("S"));
    }
}
