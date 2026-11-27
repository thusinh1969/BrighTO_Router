//! Gemini SSE repair: synthesise the `index` field on streamed tool calls.
//!
//! Gemini's OpenAI-compatible endpoint streams every parallel `tool_calls` entry
//! as its own `delta` **without** an `index`, while the OpenAI streaming schema
//! requires it to tell fragments of one call from several calls. A conformant
//! client therefore defaults every entry to index 0 and concatenates them: five
//! parallel calls come back to the router as one call named
//! `list_dirgrepread_filerun_testsgit_status` with concatenated arguments, and the
//! upstream rejects the next step with
//!
//! ```text
//! 400 INVALID_ARGUMENT "Request contains an invalid argument."
//! ```
//!
//! That is intermittent by nature — it only bites when the model answers with
//! more than one tool call — which is why it looked like random tooling failures.
//! Every other coding-agent tool call shape works, so only multi-call steps break.
//!
//! Frames are rewritten only when an index is actually missing; everything else is
//! forwarded byte for byte.

use std::collections::HashMap;

use serde_json::Value;

/// Index bookkeeping for one stream. Split out of [`ToolCallIndexer`] so `feed`
/// can mutate it while reading the line buffer (disjoint field borrows).
#[derive(Default)]
struct Numbering {
    /// `tool_call_id -> stream index`, assigned in first-seen order.
    by_id: HashMap<String, u64>,
    next_index: u64,
    /// Index most recently seen, for fragments that carry neither index nor id.
    last_index: Option<u64>,
}

/// Incremental SSE rewriter: chunks may split a line, so buffer the tail.
#[derive(Default)]
pub struct ToolCallIndexer {
    buf: Vec<u8>,
    numbering: Numbering,
}

impl ToolCallIndexer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw byte chunk and get back the bytes ready to forward.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::with_capacity(self.buf.len());
        let mut start = 0usize;
        while let Some(offset) = self.buf[start..].iter().position(|b| *b == b'\n') {
            let end = start + offset;
            out.extend_from_slice(&rewrite(&mut self.numbering, &self.buf[start..end]));
            out.push(b'\n');
            start = end + 1;
        }
        self.buf.drain(..start);
        out
    }

    /// Flush a trailing partial line at end of stream.
    pub fn finish(&mut self) -> Vec<u8> {
        let rest = std::mem::take(&mut self.buf);
        if rest.is_empty() {
            Vec::new()
        } else {
            rewrite(&mut self.numbering, &rest)
        }
    }
}

fn rewrite(numbering: &mut Numbering, line: &[u8]) -> Vec<u8> {
    let Some(rest) = line.strip_prefix(b"data:") else {
        return line.to_vec();
    };
    let payload = rest.strip_prefix(b" ").unwrap_or(rest);
    if payload.windows(6).any(|w| w == b"[DONE]") {
        return line.to_vec();
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(payload) else {
        return line.to_vec();
    };
    if !assign(numbering, &mut value) {
        return line.to_vec();
    }
    let Ok(body) = serde_json::to_vec(&value) else {
        return line.to_vec();
    };
    let mut out = Vec::with_capacity(body.len() + 6);
    out.extend_from_slice(b"data: ");
    out.extend_from_slice(&body);
    out
}

/// Number the tool calls in this frame. Returns true when an index was added.
fn assign(numbering: &mut Numbering, value: &mut Value) -> bool {
    let Some(choices) = value.get_mut("choices").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for choice in choices {
        let Some(calls) = choice
            .get_mut("delta")
            .and_then(|delta| delta.get_mut("tool_calls"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for call in calls {
            let Some(object) = call.as_object_mut() else {
                continue;
            };
            if let Some(index) = object.get("index").and_then(Value::as_u64) {
                numbering.last_index = Some(index);
                continue;
            }
            let index = match object.get("id").and_then(Value::as_str).map(str::to_owned) {
                Some(id) => match numbering.by_id.get(&id) {
                    Some(existing) => *existing,
                    None => {
                        let index = numbering.next_index;
                        numbering.next_index += 1;
                        numbering.by_id.insert(id, index);
                        index
                    }
                },
                // No id: a continuation fragment of the call already in flight.
                None => numbering.last_index.unwrap_or(0),
            };
            numbering.last_index = Some(index);
            object.insert("index".to_string(), Value::from(index));
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feed_line(indexer: &mut ToolCallIndexer, value: serde_json::Value) -> Vec<u8> {
        let mut bytes = b"data: ".to_vec();
        bytes.extend_from_slice(&serde_json::to_vec(&value).unwrap());
        bytes.push(b'\n');
        indexer.feed(&bytes)
    }

    fn parse(out: &[u8]) -> serde_json::Value {
        let text = std::str::from_utf8(out).unwrap().trim();
        serde_json::from_str(text.strip_prefix("data: ").unwrap()).unwrap()
    }

    /// The exact failure: five parallel calls with no index must get 0..4.
    #[test]
    fn numbers_parallel_calls_in_first_seen_order() {
        let mut indexer = ToolCallIndexer::new();
        for (expected, name) in ["list_dir", "grep", "read_file", "run_tests", "git_status"]
            .into_iter()
            .enumerate()
        {
            let out = feed_line(
                &mut indexer,
                json!({"choices": [{"delta": {"tool_calls": [{
                    "id": format!("call_{name}"),
                    "type": "function",
                    "function": {"name": name, "arguments": "{}"}
                }]}}]}),
            );
            let value = parse(&out);
            assert_eq!(
                value["choices"][0]["delta"]["tool_calls"][0]["index"],
                json!(expected)
            );
        }
    }

    #[test]
    fn a_frame_that_already_has_index_is_forwarded_untouched() {
        let mut indexer = ToolCallIndexer::new();
        let original = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":7,"id":"x"}]}}]}"#;
        let mut with_newline = original.to_vec();
        with_newline.push(b'\n');
        assert_eq!(indexer.feed(&with_newline), with_newline);
    }

    #[test]
    fn split_lines_are_buffered() {
        let mut indexer = ToolCallIndexer::new();
        assert!(
            indexer
                .feed(br#"data: {"choices":[{"delta":{"tool_ca"#)
                .is_empty()
        );
        let out =
            indexer.feed(br#"lls":[{"id":"a","function":{"name":"n","arguments":"{}"}}]}}]}"#);
        assert!(out.is_empty(), "no newline yet");
        let done = indexer.feed(b"\n");
        let value = parse(&done);
        assert_eq!(
            value["choices"][0]["delta"]["tool_calls"][0]["index"],
            json!(0)
        );
    }

    #[test]
    fn repeated_id_keeps_its_index() {
        let mut indexer = ToolCallIndexer::new();
        feed_line(
            &mut indexer,
            json!({"choices": [{"delta": {"tool_calls": [{"id": "dup"}]}}]}),
        );
        let out = feed_line(
            &mut indexer,
            json!({"choices": [{"delta": {"tool_calls": [
                {"id": "dup"}, {"id": "other"}
            ]}}]}),
        );
        let value = parse(&out);
        assert_eq!(
            value["choices"][0]["delta"]["tool_calls"][0]["index"],
            json!(0)
        );
        assert_eq!(
            value["choices"][0]["delta"]["tool_calls"][1]["index"],
            json!(1)
        );
    }

    #[test]
    fn a_fragment_without_id_continues_the_current_call() {
        let mut indexer = ToolCallIndexer::new();
        feed_line(
            &mut indexer,
            json!({"choices": [{"delta": {"tool_calls": [{"id": "first"}]}}]}),
        );
        let out = feed_line(
            &mut indexer,
            json!({"choices": [{"delta": {"tool_calls": [
                {"function": {"arguments": "{\"a\":1}"}}
            ]}}]}),
        );
        assert_eq!(
            parse(&out)["choices"][0]["delta"]["tool_calls"][0]["index"],
            json!(0)
        );
    }

    #[test]
    fn non_data_lines_and_done_are_untouched() {
        let mut indexer = ToolCallIndexer::new();
        let raw = b": keep-alive\n";
        assert_eq!(indexer.feed(raw), raw);
        let done = b"data: [DONE]\n";
        assert_eq!(indexer.feed(done), done);
    }

    #[test]
    fn finish_flushes_a_trailing_partial_line() {
        let mut indexer = ToolCallIndexer::new();
        let tail = br#"data: {"choices":[{"delta":{"tool_calls":[{"id":"z"}]}}]}"#;
        assert!(indexer.feed(tail).is_empty());
        let out = indexer.finish();
        assert_eq!(
            parse(&out)["choices"][0]["delta"]["tool_calls"][0]["index"],
            json!(0)
        );
    }
}
