//! OpenAI Chat Completions SSE -> Anthropic Messages SSE.
//!
//! The translator is independent of the HTTP layer so the proxy can tap the
//! original upstream bytes for usage accounting while feeding the same bytes
//! here. It accepts arbitrary chunk boundaries, emits a complete Anthropic
//! event sequence, and turns translation failures into an Anthropic `error`
//! event instead of silently ending the stream.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::response::{self, stop_reason, tool_input, usage_from};
use super::{
    BridgeContext, BridgeError, SSE_LINE_LIMIT, anthropic_sse_error, bridge_message_id, sse_event,
    thinking_signature,
};

#[derive(Default)]
struct ToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

enum OpenBlock {
    Text(u32),
    Thinking { index: u32, text: String },
}

pub struct SseTranslator {
    ctx: BridgeContext,
    buffer: Vec<u8>,
    started: bool,
    finished: bool,
    upstream_id: Option<String>,
    next_index: u32,
    open: Option<OpenBlock>,
    tools: BTreeMap<usize, ToolCallState>,
    finish_reason: Option<String>,
    usage_input: u64,
    usage_output: u64,
    emitted_block: bool,
    error: Option<BridgeError>,
}

impl SseTranslator {
    pub fn new(ctx: BridgeContext) -> Self {
        Self {
            ctx,
            buffer: Vec::new(),
            started: false,
            finished: false,
            upstream_id: None,
            next_index: 0,
            open: None,
            tools: BTreeMap::new(),
            finish_reason: None,
            usage_input: 0,
            usage_output: 0,
            emitted_block: false,
            error: None,
        }
    }

    /// Feed an arbitrary upstream chunk. Returns bytes that should be sent to
    /// the client. The translator never returns a partial HTTP response: a
    /// malformed upstream stream ends with a normal Anthropic `error` event.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }
        self.buffer.extend_from_slice(chunk);
        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            match self.process_line(&line, &mut out) {
                Ok(true) => break,
                Ok(false) => {}
                Err(error) => {
                    self.fail(&mut out, error);
                    break;
                }
            }
        }
        if self.finished {
            self.buffer.clear();
        } else if self.buffer.len() > SSE_LINE_LIMIT {
            self.fail(
                &mut out,
                BridgeError::Upstream("upstream SSE line exceeded 1 MiB".into()),
            );
        }
        out
    }

    /// Flush a trailing frame that did not end with a newline, then emit the
    /// final `message_delta` and `message_stop` events. Idempotent.
    pub fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }
        if !self.buffer.is_empty() {
            let tail = std::mem::take(&mut self.buffer);
            let text = String::from_utf8_lossy(&tail);
            for line in text.split('\n') {
                if self.finished {
                    break;
                }
                if let Err(error) = self.process_line(line, &mut out) {
                    self.fail(&mut out, error);
                    break;
                }
            }
        }
        if !self.finished
            && let Err(error) = self.finalize(&mut out)
        {
            self.fail(&mut out, error);
        }
        out
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn take_error(&mut self) -> Option<BridgeError> {
        self.error.take()
    }

    /// End a stream that failed mid-flight with an Anthropic `error` event.
    /// Used for upstream read failures and idle timeouts after headers were sent.
    pub fn abort(&mut self, status: u16, message: &str) -> Vec<u8> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }
        self.close_open(&mut out);
        out.extend(anthropic_sse_error(status, message));
        self.error = Some(BridgeError::Upstream(message.to_string()));
        self.finished = true;
        out
    }

    fn process_line(&mut self, line: &str, out: &mut Vec<u8>) -> Result<bool, BridgeError> {
        let line = line.trim_end_matches('\r');
        let data = if let Some(data) = line.strip_prefix("data:") {
            data.trim()
        } else {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                trimmed
            } else {
                return Ok(false);
            }
        };
        if data.is_empty() {
            return Ok(false);
        }
        if data == "[DONE]" {
            self.finalize(out)?;
            return Ok(true);
        }
        let value: Value = serde_json::from_str(data).map_err(|error| {
            BridgeError::Upstream(format!("invalid upstream SSE JSON: {error}"))
        })?;
        self.process_value(&value, out)?;
        Ok(self.finished)
    }

    fn process_value(&mut self, value: &Value, out: &mut Vec<u8>) -> Result<(), BridgeError> {
        let object = value
            .as_object()
            .ok_or_else(|| BridgeError::Upstream("upstream SSE chunk is not an object".into()))?;
        if let Some(error) = object.get("error").filter(|value| !value.is_null()) {
            return Err(BridgeError::Upstream(upstream_error_message(error)));
        }
        if self.upstream_id.is_none() {
            self.upstream_id = object
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        let (input, output) = usage_from(value);
        if input > 0 {
            self.usage_input = input;
        }
        if output > 0 {
            self.usage_output = output;
        }

        let Some(choices) = object.get("choices").and_then(Value::as_array) else {
            return Ok(());
        };
        let Some(choice) = choices.first().and_then(Value::as_object) else {
            return Ok(());
        };
        if let Some(delta) = choice.get("delta").and_then(Value::as_object) {
            self.process_delta(delta, out);
        } else if let Some(message) = choice.get("message").and_then(Value::as_object) {
            self.process_message(message, out);
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(reason.to_string());
            self.close_open(out);
        }
        Ok(())
    }

    fn process_delta(&mut self, delta: &Map<String, Value>, out: &mut Vec<u8>) {
        if let Some(reasoning) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(Value::as_str)
        {
            self.emit_thinking(reasoning, out);
        }
        if let Some(content) = delta.get("content") {
            self.emit_content(content, out);
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for (position, call) in calls.iter().enumerate() {
                if let Some(call) = call.as_object() {
                    self.tool_delta(call, position);
                }
            }
        }
        if let Some(call) = delta.get("function_call").and_then(Value::as_object) {
            self.legacy_tool_delta(call);
        }
    }

    fn process_message(&mut self, message: &Map<String, Value>, out: &mut Vec<u8>) {
        if let Some(reasoning) = message
            .get("reasoning_content")
            .or_else(|| message.get("reasoning"))
            .and_then(Value::as_str)
        {
            self.emit_thinking(reasoning, out);
        }
        if let Some(content) = message.get("content") {
            self.emit_content(content, out);
        }
        if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
            for (position, call) in calls.iter().enumerate() {
                if let Some(call) = call.as_object() {
                    self.tool_delta(call, position);
                }
            }
        }
        if let Some(call) = message.get("function_call").and_then(Value::as_object) {
            self.legacy_tool_delta(call);
        }
    }

    fn emit_content(&mut self, content: &Value, out: &mut Vec<u8>) {
        if let Some(text) = content.as_str() {
            self.emit_text(text, out);
            return;
        }
        if let Some(parts) = content.as_array() {
            for part in parts {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                if matches!(kind, "text" | "output_text")
                    && let Some(text) = part.get("text").and_then(Value::as_str)
                {
                    self.emit_text(text, out);
                }
            }
        }
    }

    fn emit_text(&mut self, text: &str, out: &mut Vec<u8>) {
        if text.is_empty() {
            return;
        }
        let index = self.open_text(out);
        out.extend(sse_event(
            "content_block_delta",
            &json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "text_delta", "text": text},
            }),
        ));
        self.emitted_block = true;
    }

    fn emit_thinking(&mut self, text: &str, out: &mut Vec<u8>) {
        if text.is_empty() {
            return;
        }
        let index = self.open_thinking(out);
        if let Some(OpenBlock::Thinking {
            text: accumulated, ..
        }) = &mut self.open
        {
            accumulated.push_str(text);
        }
        out.extend(sse_event(
            "content_block_delta",
            &json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "thinking_delta", "thinking": text},
            }),
        ));
        self.emitted_block = true;
    }

    fn open_text(&mut self, out: &mut Vec<u8>) -> u32 {
        if let Some(OpenBlock::Text(index)) = &self.open {
            return *index;
        }
        self.ensure_started(out);
        self.close_open(out);
        let index = self.next_index;
        self.next_index += 1;
        self.open = Some(OpenBlock::Text(index));
        out.extend(sse_event(
            "content_block_start",
            &json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {"type": "text", "text": ""},
            }),
        ));
        self.emitted_block = true;
        index
    }

    fn open_thinking(&mut self, out: &mut Vec<u8>) -> u32 {
        if let Some(OpenBlock::Thinking { index, .. }) = &self.open {
            return *index;
        }
        self.ensure_started(out);
        self.close_open(out);
        let index = self.next_index;
        self.next_index += 1;
        self.open = Some(OpenBlock::Thinking {
            index,
            text: String::new(),
        });
        out.extend(sse_event(
            "content_block_start",
            &json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {"type": "thinking", "thinking": ""},
            }),
        ));
        self.emitted_block = true;
        index
    }

    fn close_open(&mut self, out: &mut Vec<u8>) {
        let Some(block) = self.open.take() else {
            return;
        };
        match block {
            OpenBlock::Text(index) => {
                out.extend(sse_event(
                    "content_block_stop",
                    &json!({"type": "content_block_stop", "index": index}),
                ));
            }
            OpenBlock::Thinking { index, text } => {
                out.extend(sse_event(
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {
                            "type": "signature_delta",
                            "signature": thinking_signature(&text),
                        },
                    }),
                ));
                out.extend(sse_event(
                    "content_block_stop",
                    &json!({"type": "content_block_stop", "index": index}),
                ));
            }
        }
    }

    fn tool_delta(&mut self, call: &Map<String, Value>, position: usize) {
        let index = call
            .get("index")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(position);
        let state = self.tools.entry(index).or_default();
        if state.id.is_none() {
            state.id = call
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(function) = call.get("function").and_then(Value::as_object) {
            if state.name.is_none() {
                state.name = function
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
            }
            if let Some(arguments) = function.get("arguments") {
                append_tool_arguments(state, arguments);
            }
        } else if state.name.is_none() {
            state.name = call
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
    }

    fn legacy_tool_delta(&mut self, call: &Map<String, Value>) {
        let state = self.tools.entry(0).or_default();
        if state.name.is_none() {
            state.name = call
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(arguments) = call.get("arguments") {
            append_tool_arguments(state, arguments);
        }
    }

    fn ensure_started(&mut self, out: &mut Vec<u8>) {
        if self.started {
            return;
        }
        let id = bridge_message_id(&self.ctx.request_id, self.upstream_id.as_deref());
        let input_tokens = if self.usage_input > 0 {
            self.usage_input
        } else {
            self.ctx.estimated_input_tokens
        };
        let message = json!({
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": self.ctx.public_model.clone(),
            "content": [],
            "stop_reason": Value::Null,
            "stop_sequence": Value::Null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": 0,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
            },
        });
        out.extend(sse_event(
            "message_start",
            &json!({"type": "message_start", "message": message}),
        ));
        self.started = true;
    }

    fn finalize(&mut self, out: &mut Vec<u8>) -> Result<(), BridgeError> {
        if self.finished {
            return Ok(());
        }
        self.ensure_started(out);
        self.close_open(out);
        let has_tools = !self.tools.is_empty();
        let tools = std::mem::take(&mut self.tools);
        for (order, (_, state)) in tools.into_iter().enumerate() {
            let name = state.name.ok_or_else(|| {
                BridgeError::Upstream("upstream tool call is missing a function name".into())
            })?;
            let input = tool_input(Some(&Value::String(state.arguments.clone())))?;
            let id = state
                .id
                .unwrap_or_else(|| response::synthetic_tool_id(&self.ctx.request_id, order));
            let index = self.next_index;
            self.next_index += 1;
            out.extend(sse_event(
                "content_block_start",
                &json!({
                    "type": "content_block_start",
                    "index": index,
                    "content_block": {
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": {},
                    },
                }),
            ));
            let partial_json = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
            out.extend(sse_event(
                "content_block_delta",
                &json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {"type": "input_json_delta", "partial_json": partial_json},
                }),
            ));
            out.extend(sse_event(
                "content_block_stop",
                &json!({"type": "content_block_stop", "index": index}),
            ));
            self.emitted_block = true;
        }
        if !self.emitted_block {
            let index = self.next_index;
            self.next_index += 1;
            out.extend(sse_event(
                "content_block_start",
                &json!({
                    "type": "content_block_start",
                    "index": index,
                    "content_block": {"type": "text", "text": ""},
                }),
            ));
            out.extend(sse_event(
                "content_block_stop",
                &json!({"type": "content_block_stop", "index": index}),
            ));
            self.emitted_block = true;
        }
        out.extend(sse_event(
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason(self.finish_reason.as_deref(), has_tools),
                    "stop_sequence": Value::Null,
                },
                "usage": {"output_tokens": self.usage_output},
            }),
        ));
        out.extend(sse_event("message_stop", &json!({"type": "message_stop"})));
        self.finished = true;
        Ok(())
    }

    fn fail(&mut self, out: &mut Vec<u8>, error: BridgeError) {
        if self.finished {
            return;
        }
        self.close_open(out);
        out.extend(anthropic_sse_error(error.status(), &error.message()));
        self.error = Some(error);
        self.finished = true;
    }
}

fn append_tool_arguments(state: &mut ToolCallState, arguments: &Value) {
    match arguments {
        Value::String(text) => state.arguments.push_str(text),
        Value::Object(_) => {
            if let Ok(encoded) = serde_json::to_string(arguments) {
                state.arguments = encoded;
            }
        }
        _ => {}
    }
}

fn upstream_error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .unwrap_or("upstream returned an error")
        .to_string()
}

/// Build a complete Anthropic SSE sequence from one already-translated
/// non-stream message. Used when an upstream ignores `stream: true` but still
/// labels the response as SSE, or when the proxy receives JSON on a stream.
pub fn message_to_sse(message: &Value, ctx: &BridgeContext) -> Vec<u8> {
    let mut out = Vec::new();
    let id = message
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| bridge_message_id(&ctx.request_id, None));
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&ctx.public_model);
    let usage = message.get("usage");
    let input_tokens = usage
        .and_then(|usage| usage.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(ctx.estimated_input_tokens);
    let output_tokens = usage
        .and_then(|usage| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let start = json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [],
        "stop_reason": Value::Null,
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": 0,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
        },
    });
    out.extend(sse_event(
        "message_start",
        &json!({"type": "message_start", "message": start}),
    ));

    let mut index = 0u32;
    let mut emitted = false;
    if let Some(blocks) = message.get("content").and_then(Value::as_array) {
        for block in blocks {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                    out.extend(sse_event(
                        "content_block_start",
                        &json!({
                            "type": "content_block_start",
                            "index": index,
                            "content_block": {"type": "text", "text": ""},
                        }),
                    ));
                    if !text.is_empty() {
                        out.extend(sse_event(
                            "content_block_delta",
                            &json!({
                                "type": "content_block_delta",
                                "index": index,
                                "delta": {"type": "text_delta", "text": text},
                            }),
                        ));
                    }
                    out.extend(sse_event(
                        "content_block_stop",
                        &json!({"type": "content_block_stop", "index": index}),
                    ));
                    index += 1;
                    emitted = true;
                }
                "thinking" => {
                    let thinking = block.get("thinking").and_then(Value::as_str).unwrap_or("");
                    let signature = block
                        .get("signature")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| thinking_signature(thinking));
                    out.extend(sse_event(
                        "content_block_start",
                        &json!({
                            "type": "content_block_start",
                            "index": index,
                            "content_block": {"type": "thinking", "thinking": ""},
                        }),
                    ));
                    if !thinking.is_empty() {
                        out.extend(sse_event(
                            "content_block_delta",
                            &json!({
                                "type": "content_block_delta",
                                "index": index,
                                "delta": {"type": "thinking_delta", "thinking": thinking},
                            }),
                        ));
                    }
                    out.extend(sse_event(
                        "content_block_delta",
                        &json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {"type": "signature_delta", "signature": signature},
                        }),
                    ));
                    out.extend(sse_event(
                        "content_block_stop",
                        &json!({"type": "content_block_stop", "index": index}),
                    ));
                    index += 1;
                    emitted = true;
                }
                "tool_use" => {
                    let tool_id = block.get("id").and_then(Value::as_str).unwrap_or("");
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                    out.extend(sse_event(
                        "content_block_start",
                        &json!({
                            "type": "content_block_start",
                            "index": index,
                            "content_block": {
                                "type": "tool_use",
                                "id": tool_id,
                                "name": name,
                                "input": {},
                            },
                        }),
                    ));
                    let partial_json =
                        serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                    out.extend(sse_event(
                        "content_block_delta",
                        &json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {"type": "input_json_delta", "partial_json": partial_json},
                        }),
                    ));
                    out.extend(sse_event(
                        "content_block_stop",
                        &json!({"type": "content_block_stop", "index": index}),
                    ));
                    index += 1;
                    emitted = true;
                }
                _ => {}
            }
        }
    }
    if !emitted {
        out.extend(sse_event(
            "content_block_start",
            &json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""},
            }),
        ));
        out.extend(sse_event(
            "content_block_stop",
            &json!({"type": "content_block_stop", "index": 0}),
        ));
    }
    let reason = message
        .get("stop_reason")
        .and_then(Value::as_str)
        .unwrap_or("end_turn");
    out.extend(sse_event(
        "message_delta",
        &json!({
            "type": "message_delta",
            "delta": {"stop_reason": reason, "stop_sequence": Value::Null},
            "usage": {"output_tokens": output_tokens},
        }),
    ));
    out.extend(sse_event("message_stop", &json!({"type": "message_stop"})));
    out
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

    fn frame(value: Value) -> String {
        format!("data: {value}\n\n")
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).to_string()
    }

    #[test]
    fn translates_text_and_usage() {
        let mut translator = SseTranslator::new(ctx());
        let mut out = Vec::new();
        out.extend(
            translator.feed(
                frame(json!({
                    "id": "chatcmpl-1",
                    "choices": [{"delta": {"content": "Hel"}}],
                }))
                .as_bytes(),
            ),
        );
        out.extend(
            translator.feed(
                frame(json!({
                    "choices": [{"delta": {"content": "lo"}, "finish_reason": "stop"}],
                }))
                .as_bytes(),
            ),
        );
        out.extend(
            translator.feed(
                frame(json!({
                    "choices": [],
                    "usage": {"prompt_tokens": 5, "completion_tokens": 2},
                }))
                .as_bytes(),
            ),
        );
        out.extend(translator.feed(b"data: [DONE]\n\n"));
        let out = text(&out);
        assert!(out.contains("event: message_start"));
        assert!(out.contains("\"id\":\"chatcmpl-1\""));
        assert!(out.contains("\"text\":\"Hel\""));
        assert!(out.contains("\"text\":\"lo\""));
        assert!(out.contains("\"stop_reason\":\"end_turn\""));
        assert!(out.contains("\"output_tokens\":2"));
        assert!(out.contains("event: message_stop"));
        assert!(!out.contains("[DONE]"));
        assert!(!out.contains("event: error"));
        assert!(translator.is_finished());
        assert!(translator.take_error().is_none());
    }

    #[test]
    fn buffers_fragmented_tool_arguments_until_finish() {
        let mut translator = SseTranslator::new(ctx());
        let mut out = Vec::new();
        out.extend(
            translator.feed(
                frame(json!({
                    "choices": [{"delta": {"tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {"name": "Read", "arguments": "{\"path\":"}
                    }]}}],
                }))
                .as_bytes(),
            ),
        );
        out.extend(
            translator.feed(
                frame(json!({
                    "choices": [{"delta": {"tool_calls": [{
                        "index": 0,
                        "function": {"arguments": "\"a.txt\"}"}
                    }]}}],
                }))
                .as_bytes(),
            ),
        );
        out.extend(
            translator.feed(
                frame(json!({
                    "choices": [{"delta": {}, "finish_reason": "tool_calls"}],
                }))
                .as_bytes(),
            ),
        );
        out.extend(translator.feed(b"data: [DONE]\n\n"));
        let out = text(&out);
        assert!(out.contains("\"type\":\"tool_use\""));
        assert!(out.contains("\"id\":\"call_1\""));
        assert!(out.contains("\"name\":\"Read\""));
        assert!(out.contains("input_json_delta"));
        assert!(out.contains("{\\\"path\\\":\\\"a.txt\\\"}"));
        assert!(out.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn emits_thinking_with_signature() {
        let mut translator = SseTranslator::new(ctx());
        let mut out = translator.feed(
            frame(json!({
                "choices": [{"delta": {"reasoning_content": "why"}}],
            }))
            .as_bytes(),
        );
        out.extend(translator.finish());
        let out = text(&out);
        assert!(out.contains("thinking_delta"));
        assert!(out.contains("signature_delta"));
        assert!(out.contains("brighto-bridge-"));
    }

    #[test]
    fn malformed_upstream_becomes_an_error_event() {
        let mut translator = SseTranslator::new(ctx());
        let out = text(&translator.feed(frame(json!({"error": {"message": "boom"}})).as_bytes()));
        assert!(out.contains("event: error"));
        assert!(out.contains("boom"));
        assert!(translator.is_finished());
        assert!(matches!(
            translator.take_error(),
            Some(BridgeError::Upstream(_))
        ));
    }

    #[test]
    fn accepts_arbitrary_chunk_boundaries() {
        let full = frame(json!({
            "choices": [{"delta": {"content": "split"}}],
        }));
        let mut translator = SseTranslator::new(ctx());
        let mut out = Vec::new();
        for byte in full.as_bytes() {
            out.extend(translator.feed(&[*byte]));
        }
        out.extend(translator.finish());
        let out = text(&out);
        assert!(out.contains("\"text\":\"split\""));
        assert!(out.contains("event: message_stop"));
    }

    #[test]
    fn synthesizes_sse_from_a_complete_message() {
        let message = json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "public-model",
            "content": [
                {"type": "text", "text": "hi"},
                {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"path": "a"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 3, "output_tokens": 4}
        });
        let out = text(&message_to_sse(&message, &ctx()));
        assert!(out.contains("event: message_start"));
        assert!(out.contains("\"text\":\"hi\""));
        assert!(out.contains("\"id\":\"toolu_1\""));
        assert!(out.contains("\"output_tokens\":4"));
        assert!(out.contains("\"stop_reason\":\"tool_use\""));
    }
}
