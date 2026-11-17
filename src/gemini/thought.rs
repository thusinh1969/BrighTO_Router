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
//! `extra_content`, so the signature is lost. The router therefore keeps a
//! `tool_call_id -> signature` cache, records it whenever a Gemini response goes
//! by, and re-injects it into the last assistant tool call of the next request.
//!
//! The cache is **persisted** to `$DATA_DIR/thought_signatures.jsonl` (mode 0600)
//! so a router restart does not strand an in-flight agent turn, and eviction is
//! oldest-first instead of dropping the whole map.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// Cap the cache so a long-running router cannot grow without bound.
const MAX_ENTRIES: usize = 4096;
/// Rewrite the journal once it holds this many lines (compaction).
const COMPACT_AT: usize = MAX_ENTRIES * 2;

fn cache_path() -> PathBuf {
    let dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "/var/lib/brighto-router".to_string());
    PathBuf::from(dir).join("thought_signatures.jsonl")
}

struct Store {
    map: HashMap<String, String>,
    /// Insertion order, used for oldest-first eviction.
    order: VecDeque<String>,
    loaded: bool,
    lines: usize,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        Mutex::new(Store {
            map: HashMap::new(),
            order: VecDeque::new(),
            loaded: false,
            lines: 0,
        })
    })
}

fn line_for(id: &str, signature: &str) -> String {
    serde_json::json!({ "id": id, "sig": signature }).to_string()
}

/// Load the journal once, so a restart keeps signatures recorded before it.
fn ensure_loaded(st: &mut Store) {
    if st.loaded {
        return;
    }
    st.loaded = true;
    let Ok(text) = fs::read_to_string(cache_path()) else {
        return;
    };
    for line in text.lines() {
        st.lines += 1;
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let (Some(id), Some(sig)) = (
            v.get("id").and_then(|x| x.as_str()),
            v.get("sig").and_then(|x| x.as_str()),
        ) else {
            continue;
        };
        if st.map.insert(id.to_string(), sig.to_string()).is_none() {
            st.order.push_back(id.to_string());
        }
    }
    while st.map.len() > MAX_ENTRIES {
        evict_oldest(st);
    }
}

fn evict_oldest(st: &mut Store) {
    while let Some(old) = st.order.pop_front() {
        if st.map.remove(&old).is_some() {
            return;
        }
    }
}

fn push(st: &mut Store, id: &str, signature: &str) {
    if st
        .map
        .insert(id.to_string(), signature.to_string())
        .is_none()
    {
        st.order.push_back(id.to_string());
    }
    while st.map.len() > MAX_ENTRIES {
        evict_oldest(st);
    }
}

fn rewrite_journal(st: &mut Store) {
    let mut out = String::new();
    for (id, sig) in &st.map {
        out.push_str(&line_for(id, sig));
        out.push('\n');
    }
    let path = cache_path();
    let tmp = path.with_extension("jsonl.tmp");
    if fs::write(&tmp, out).is_ok() {
        let _ = fs::set_permissions(&tmp, std::os::unix::fs::PermissionsExt::from_mode(0o600));
        if fs::rename(&tmp, &path).is_ok() {
            st.lines = st.map.len();
        }
    }
}

/// Remember the signature of one tool call (and persist it).
pub fn record(id: &str, signature: &str) {
    if id.is_empty() || signature.is_empty() {
        return;
    }
    let Ok(mut st) = store().lock() else {
        return;
    };
    ensure_loaded(&mut st);
    push(&mut st, id, signature);

    let path = cache_path();
    let is_new = !path.exists();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", line_for(id, signature));
        let _ = f.flush();
        if is_new {
            let _ = fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600));
        }
        st.lines += 1;
    }
    if st.lines > COMPACT_AT {
        rewrite_journal(&mut st);
    }
}

/// Signature previously seen for this tool call id.
pub fn lookup(id: &str) -> Option<String> {
    let mut st = store().lock().ok()?;
    ensure_loaded(&mut st);
    st.map.get(id).cloned()
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

    #[test]
    fn eviction_is_oldest_first_and_bounded() {
        let mut st = Store {
            map: HashMap::new(),
            order: VecDeque::new(),
            loaded: true,
            lines: 0,
        };
        for i in 0..MAX_ENTRIES + 10 {
            push(&mut st, &format!("id{i}"), "sig");
        }
        assert_eq!(st.map.len(), MAX_ENTRIES);
        assert!(!st.map.contains_key("id0"));
        assert!(st.map.contains_key(&format!("id{}", MAX_ENTRIES + 9)));
    }
}
