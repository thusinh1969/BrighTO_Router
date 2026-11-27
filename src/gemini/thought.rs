//! Gemini thought-signature carry-over.
//!
//! With tools enabled, Gemini binds a returned `functionCall` to an opaque
//! `thought_signature` and **requires it back** on the next turn:
//!
//! ```text
//! 400 Function call is missing a thought_signature in functionCall parts.
//! ... function call `default_api:read_file`, position 2.
//! ```
//!
//! The carrier is the **first tool call of each step**, not the last one and not
//! every call. Per Google's thought-signature rules:
//!
//! * a step with one function call -> that call carries the signature;
//! * parallel calls in one response -> only the FIRST carries it; the siblings
//!   legitimately have none, and must not be given a made-up one;
//! * a multi-step tool loop -> EVERY step's first call carries its own signature,
//!   and the next request has to replay all of them. Signing only the newest step
//!   is what produced `position 2` on the older one.
//!
//! Generic OpenAI clients — coding agents in particular — rebuild `tool_calls`
//! from id/name/arguments and drop `extra_content`, so the router keeps a
//! `tool_call_id -> signature` cache, records every signature a Gemini response
//! carries (both streaming and non-streaming, in
//! `tool_calls[].extra_content.google.thought_signature`), and re-signs each
//! step's first call on the way back. When a call was never seen — client-built
//! or cross-model history — it uses the dummy Google documents for exactly this
//! case, [`MISSING_SIGNATURE`], instead of letting the request 400.
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

/// Google's documented placeholder for a function call that legitimately has no
/// signature: history built by the client, or a tool loop migrated from another
/// model. Sent instead of dropping the call, which would break the
/// functionCall/functionResponse pairing.
pub const MISSING_SIGNATURE: &str = "skip_thought_signature_validator";

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

/// Re-sign every assistant step in a request. Returns true when something changed.
///
/// Only the first tool call of a step carries a signature; parallel siblings keep
/// exactly what the client sent (normally nothing). A step whose first call was
/// never recorded gets [`MISSING_SIGNATURE`].
pub fn inject_into_request(value: &mut Value) -> bool {
    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let mut changed = false;
    for message in messages.iter_mut() {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(first) = message
            .get_mut("tool_calls")
            .and_then(|v| v.as_array_mut())
            .and_then(|calls| calls.first_mut())
        else {
            continue;
        };
        // The client replayed a real signature; leave it untouched.
        if signature_of(first).is_some() {
            continue;
        }
        let signature = first
            .get("id")
            .and_then(Value::as_str)
            .and_then(lookup)
            .unwrap_or_else(|| MISSING_SIGNATURE.to_string());
        let Some(object) = first.as_object_mut() else {
            continue;
        };
        object.insert(
            "extra_content".to_string(),
            serde_json::json!({"google": {"thought_signature": signature}}),
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
    fn records_and_injects_the_step_signature() {
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

    /// A multi-step tool loop needs EVERY step signed. Signing only the newest
    /// step is what upstream rejected with `position 2`.
    #[test]
    fn signs_the_first_call_of_every_step() {
        let resp = json!({"choices": [{"message": {"tool_calls": [
            {"id": "step_1", "extra_content": {"google": {"thought_signature": "S1"}}},
            {"id": "step_2", "extra_content": {"google": {"thought_signature": "S2"}}}
        ]}}]});
        record_from_response(&resp);
        let mut req = json!({"messages": [
            {"role": "user", "content": "loop"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "step_1", "type": "function", "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "step_1", "content": "{}"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "step_2", "type": "function", "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "step_2", "content": "{}"}
        ]});
        assert!(inject_into_request(&mut req));
        assert_eq!(
            req["messages"][1]["tool_calls"][0]["extra_content"]["google"]["thought_signature"],
            json!("S1")
        );
        assert_eq!(
            req["messages"][3]["tool_calls"][0]["extra_content"]["google"]["thought_signature"],
            json!("S2")
        );
    }

    /// Client-built or cross-model history has no signature to replay; Google
    /// documents a dummy so the call is not rejected outright.
    #[test]
    fn unknown_calls_get_the_documented_dummy() {
        let mut req = json!({"messages": [
            {"role": "assistant", "content": null,
             "tool_calls": [{"id": "call_unknown", "type": "function",
                             "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "call_unknown", "content": "{}"}
        ]});
        assert!(inject_into_request(&mut req));
        assert_eq!(
            req["messages"][0]["tool_calls"][0]["extra_content"]["google"]["thought_signature"],
            json!(MISSING_SIGNATURE)
        );
    }

    /// Parallel calls share one signature, held by the first one only.
    #[test]
    fn parallel_siblings_are_not_signed() {
        record("call_par_first", "PAR");
        let mut req = json!({"messages": [
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_par_first", "type": "function", "function": {"name": "a", "arguments": "{}"}},
                {"id": "call_par_second", "type": "function", "function": {"name": "b", "arguments": "{}"}},
                {"id": "call_par_third", "type": "function", "function": {"name": "c", "arguments": "{}"}}
            ]}
        ]});
        assert!(inject_into_request(&mut req));
        let calls = req["messages"][0]["tool_calls"].as_array().unwrap();
        assert_eq!(
            calls[0]["extra_content"]["google"]["thought_signature"],
            json!("PAR")
        );
        assert!(calls[1].get("extra_content").is_none());
        assert!(calls[2].get("extra_content").is_none());
    }

    /// A signature the client already replayed wins over the cache.
    #[test]
    fn replayed_signatures_are_left_alone() {
        record("call_kept", "CACHED");
        let mut req = json!({"messages": [
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_kept", "type": "function", "function": {"name": "f", "arguments": "{}"},
                 "extra_content": {"google": {"thought_signature": "FROM_CLIENT"}}}]}
        ]});
        assert!(!inject_into_request(&mut req));
        assert_eq!(
            req["messages"][0]["tool_calls"][0]["extra_content"]["google"]["thought_signature"],
            json!("FROM_CLIENT")
        );
    }

    /// Text-only assistants and tool results are not steps to sign.
    #[test]
    fn messages_without_tool_calls_are_ignored() {
        let mut req = json!({"messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"},
            {"role": "tool", "tool_call_id": "x", "content": "{}"}
        ]});
        assert!(!inject_into_request(&mut req));
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
