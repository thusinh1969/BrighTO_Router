//! Anthropic Messages request -> OpenAI Chat Completions request.
//!
//! This is intentionally a fresh JSON object; we never mutate or forward the
//! Anthropic request. Unknown Anthropic-only fields are dropped, while content
//! that cannot be represented safely in OpenAI Chat returns a clear error.

use serde_json::{Map, Value, json};

use super::BridgeError;

pub fn anthropic_to_openai(body: &[u8], public_model: &str) -> Result<Vec<u8>, BridgeError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| BridgeError::InvalidRequest(format!("invalid JSON: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| BridgeError::InvalidRequest("request body must be a JSON object".into()))?;

    let messages = object
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| BridgeError::InvalidRequest("messages must be an array".into()))?;
    if messages.is_empty() {
        return Err(BridgeError::InvalidRequest(
            "messages must contain at least one message".into(),
        ));
    }
    let max_tokens = object
        .get("max_tokens")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            BridgeError::InvalidRequest("max_tokens must be a positive integer".into())
        })?;

    let mut out = Map::new();
    out.insert("model".into(), Value::String(public_model.to_string()));
    out.insert("max_tokens".into(), Value::from(max_tokens));
    let stream = object
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    out.insert("stream".into(), Value::Bool(stream));
    if stream {
        out.insert("stream_options".into(), json!({"include_usage": true}));
    }

    // OpenAI-compatible chat templates (notably strict Qwen templates) reject
    // system messages that are not first. Collect Anthropic top-level system
    // text and any system-role messages into one leading system message.
    let mut system_parts = Vec::new();
    if let Some(system) = parse_system(object.get("system"))?
        && !system.trim().is_empty()
    {
        system_parts.push(system);
    }
    let mut openai_messages = Vec::with_capacity(messages.len() + 1);
    for message in messages {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        if role == "system" {
            let content = message
                .get("content")
                .ok_or_else(|| BridgeError::InvalidRequest("message content is missing".into()))?;
            if let Some(text) = system_message_text(content)?
                && !text.trim().is_empty()
            {
                system_parts.push(text);
            }
            continue;
        }
        convert_message(message, &mut openai_messages)?;
    }
    if openai_messages.is_empty() {
        return Err(BridgeError::InvalidRequest(
            "request contains no convertible messages".into(),
        ));
    }
    if !system_parts.is_empty() {
        openai_messages.insert(
            0,
            json!({"role": "system", "content": system_parts.join("\n\n")}),
        );
    }
    out.insert("messages".into(), Value::Array(openai_messages));

    if let Some(tools) = object.get("tools").filter(|value| !value.is_null()) {
        out.insert("tools".into(), convert_tools(tools)?);
    }
    if let Some(tool_choice) = object.get("tool_choice").filter(|value| !value.is_null()) {
        let (choice, parallel) = convert_tool_choice(tool_choice)?;
        out.insert("tool_choice".into(), choice);
        if let Some(parallel) = parallel {
            out.insert("parallel_tool_calls".into(), Value::Bool(parallel));
        }
    }
    copy_number(object, "temperature", &mut out);
    copy_number(object, "top_p", &mut out);
    if let Some(stop) = object.get("stop_sequences").and_then(Value::as_array) {
        let stops: Vec<Value> = stop
            .iter()
            .filter_map(Value::as_str)
            .map(|value| Value::String(value.to_string()))
            .collect();
        if !stops.is_empty() {
            out.insert("stop".into(), Value::Array(stops));
        }
    }
    if let Some(user) = object
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        out.insert("user".into(), Value::String(user.to_string()));
    }

    serde_json::to_vec(&Value::Object(out))
        .map_err(|error| BridgeError::InvalidRequest(format!("could not encode request: {error}")))
}

fn parse_system(system: Option<&Value>) -> Result<Option<String>, BridgeError> {
    let Some(system) = system.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    if let Some(text) = system.as_str() {
        return Ok(Some(text.to_string()));
    }
    let blocks = system.as_array().ok_or_else(|| {
        BridgeError::InvalidRequest("system must be a string or an array of text blocks".into())
    })?;
    let mut parts = Vec::new();
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| BridgeError::InvalidRequest("system block must be an object".into()))?;
        expect_block_type(object, "system")?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| BridgeError::InvalidRequest("system block type is missing".into()))?;
        if kind != "text" {
            return Err(BridgeError::Unsupported(format!(
                "system block type '{kind}' is not supported"
            )));
        }
        let text = object.get("text").and_then(Value::as_str).ok_or_else(|| {
            BridgeError::InvalidRequest("system text block is missing text".into())
        })?;
        parts.push(text.to_string());
    }
    Ok(Some(parts.join("\n\n")))
}

fn convert_message(message: &Value, out: &mut Vec<Value>) -> Result<(), BridgeError> {
    let object = message
        .as_object()
        .ok_or_else(|| BridgeError::InvalidRequest("message must be an object".into()))?;
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::InvalidRequest("message role is missing".into()))?;
    let content = object
        .get("content")
        .ok_or_else(|| BridgeError::InvalidRequest("message content is missing".into()))?;

    match role {
        "user" => convert_user_message(content, out),
        "assistant" => convert_assistant_message(content, out),
        // System-role messages are collected by anthropic_to_openai and moved
        // to the leading system message so strict OpenAI chat templates accept
        // them. Reaching this arm means the role check above missed it.
        "system" => Err(BridgeError::InvalidRequest(
            "system-role message must be handled before conversion".into(),
        )),
        other => Err(BridgeError::InvalidRequest(format!(
            "unsupported message role '{other}'"
        ))),
    }
}

fn system_message_text(content: &Value) -> Result<Option<String>, BridgeError> {
    let text = if let Some(text) = content.as_str() {
        text.to_string()
    } else {
        let blocks = content.as_array().ok_or_else(|| {
            BridgeError::InvalidRequest("system content must be a string or an array".into())
        })?;
        let mut parts = Vec::new();
        for block in blocks {
            let object = block.as_object().ok_or_else(|| {
                BridgeError::InvalidRequest("system content block must be an object".into())
            })?;
            let kind = object.get("type").and_then(Value::as_str).ok_or_else(|| {
                BridgeError::InvalidRequest("system content block type is missing".into())
            })?;
            if kind != "text" {
                return Err(BridgeError::Unsupported(format!(
                    "system-role content block type '{kind}' is not supported"
                )));
            }
            let text = object.get("text").and_then(Value::as_str).ok_or_else(|| {
                BridgeError::InvalidRequest("system text block is missing text".into())
            })?;
            parts.push(text.to_string());
        }
        parts.join("\n\n")
    };
    Ok(if text.trim().is_empty() {
        None
    } else {
        Some(text)
    })
}

fn convert_user_message(content: &Value, out: &mut Vec<Value>) -> Result<(), BridgeError> {
    if let Some(text) = content.as_str() {
        out.push(json!({"role": "user", "content": text}));
        return Ok(());
    }
    let blocks = content.as_array().ok_or_else(|| {
        BridgeError::InvalidRequest("user content must be a string or an array".into())
    })?;
    let mut tool_messages = Vec::new();
    let mut parts = Vec::new();
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| BridgeError::InvalidRequest("content block must be an object".into()))?;
        expect_block_type(object, "user")?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| BridgeError::InvalidRequest("content block type is missing".into()))?;
        match kind {
            "text" => {
                let text = object.get("text").and_then(Value::as_str).ok_or_else(|| {
                    BridgeError::InvalidRequest("text block is missing text".into())
                })?;
                parts.push(json!({"type": "text", "text": text}));
            }
            "image" => parts.push(convert_image(object)?),
            "tool_result" => tool_messages.push(convert_tool_result(object)?),
            // Thinking is model-private and has no OpenAI Chat equivalent on input.
            "thinking" | "redacted_thinking" => {}
            other => {
                return Err(BridgeError::Unsupported(format!(
                    "user content block type '{other}' is not supported"
                )));
            }
        }
    }
    out.extend(tool_messages);
    if !parts.is_empty() {
        if parts.len() == 1
            && let Some(text) = parts[0].get("text").and_then(Value::as_str)
        {
            out.push(json!({"role": "user", "content": text}));
            return Ok(());
        }
        out.push(json!({"role": "user", "content": parts}));
    }
    Ok(())
}

fn convert_assistant_message(content: &Value, out: &mut Vec<Value>) -> Result<(), BridgeError> {
    if let Some(text) = content.as_str() {
        if !text.is_empty() {
            out.push(json!({"role": "assistant", "content": text}));
        }
        return Ok(());
    }
    let blocks = content.as_array().ok_or_else(|| {
        BridgeError::InvalidRequest("assistant content must be a string or an array".into())
    })?;
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for block in blocks {
        let object = block
            .as_object()
            .ok_or_else(|| BridgeError::InvalidRequest("content block must be an object".into()))?;
        expect_block_type(object, "assistant")?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| BridgeError::InvalidRequest("content block type is missing".into()))?;
        match kind {
            "text" => {
                let text = object.get("text").and_then(Value::as_str).ok_or_else(|| {
                    BridgeError::InvalidRequest("text block is missing text".into())
                })?;
                text_parts.push(text.to_string());
            }
            "tool_use" => {
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| BridgeError::InvalidRequest("tool_use id is missing".into()))?;
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        BridgeError::InvalidRequest("tool_use name is missing".into())
                    })?;
                let input = object
                    .get("input")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        BridgeError::InvalidRequest("tool_use input must be an object".into())
                    })?;
                let arguments =
                    serde_json::to_string(&Value::Object(input.clone())).map_err(|error| {
                        BridgeError::InvalidRequest(format!(
                            "could not encode tool_use input: {error}"
                        ))
                    })?;
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments},
                }));
            }
            // Thinking blocks in history are dropped; the response side is
            // best-effort, but the upstream has no signed thinking input.
            "thinking" | "redacted_thinking" => {}
            other => {
                return Err(BridgeError::Unsupported(format!(
                    "assistant content block type '{other}' is not supported"
                )));
            }
        }
    }
    if text_parts.is_empty() && tool_calls.is_empty() {
        return Ok(());
    }
    let text = if text_parts.is_empty() {
        Value::Null
    } else {
        Value::String(text_parts.join("\n\n"))
    };
    let mut message = Map::new();
    message.insert("role".into(), Value::String("assistant".into()));
    message.insert("content".into(), text);
    if !tool_calls.is_empty() {
        message.insert("tool_calls".into(), Value::Array(tool_calls));
    }
    out.push(Value::Object(message));
    Ok(())
}

fn convert_tool_result(object: &Map<String, Value>) -> Result<Value, BridgeError> {
    let tool_use_id = object
        .get("tool_use_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BridgeError::InvalidRequest("tool_result tool_use_id is missing".into()))?;
    let is_error = object
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let content = object
        .get("content")
        .ok_or_else(|| BridgeError::InvalidRequest("tool_result content is missing".into()))?;
    let content = convert_tool_result_content(content, is_error)?;
    Ok(json!({
        "role": "tool",
        "tool_call_id": tool_use_id,
        "content": content,
    }))
}

fn convert_tool_result_content(content: &Value, is_error: bool) -> Result<Value, BridgeError> {
    if let Some(text) = content.as_str() {
        return Ok(Value::String(if is_error {
            format!("Error: {text}")
        } else {
            text.to_string()
        }));
    }
    let blocks = content.as_array().ok_or_else(|| {
        BridgeError::InvalidRequest("tool_result content must be a string or an array".into())
    })?;
    let mut parts = Vec::new();
    if is_error {
        parts.push(json!({"type": "text", "text": "Error:"}));
    }
    for block in blocks {
        let object = block.as_object().ok_or_else(|| {
            BridgeError::InvalidRequest("tool_result block must be an object".into())
        })?;
        let kind = object.get("type").and_then(Value::as_str).ok_or_else(|| {
            BridgeError::InvalidRequest("tool_result block type is missing".into())
        })?;
        match kind {
            "text" => {
                let text = object.get("text").and_then(Value::as_str).ok_or_else(|| {
                    BridgeError::InvalidRequest("tool_result text block is missing text".into())
                })?;
                parts.push(json!({"type": "text", "text": text}));
            }
            "image" => parts.push(convert_image(object)?),
            other => {
                return Err(BridgeError::Unsupported(format!(
                    "tool_result block type '{other}' is not supported"
                )));
            }
        }
    }
    if parts.len() == 1
        && let Some(text) = parts[0].get("text").and_then(Value::as_str)
    {
        return Ok(Value::String(text.to_string()));
    }
    Ok(Value::Array(parts))
}

fn convert_image(object: &Map<String, Value>) -> Result<Value, BridgeError> {
    let source = object
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| BridgeError::InvalidRequest("image source is missing".into()))?;
    let kind = source
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::InvalidRequest("image source type is missing".into()))?;
    let url = match kind {
        "base64" => {
            let media_type = source
                .get("media_type")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BridgeError::InvalidRequest("base64 image media_type is missing".into())
                })?;
            let data = source.get("data").and_then(Value::as_str).ok_or_else(|| {
                BridgeError::InvalidRequest("base64 image data is missing".into())
            })?;
            format!("data:{media_type};base64,{data}")
        }
        "url" => source
            .get("url")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BridgeError::InvalidRequest("image url is missing".into()))?
            .to_string(),
        other => {
            return Err(BridgeError::Unsupported(format!(
                "image source type '{other}' is not supported"
            )));
        }
    };
    Ok(json!({"type": "image_url", "image_url": {"url": url}}))
}

fn convert_tools(tools: &Value) -> Result<Value, BridgeError> {
    let tools = tools
        .as_array()
        .ok_or_else(|| BridgeError::InvalidRequest("tools must be an array".into()))?;
    let mut out = Vec::with_capacity(tools.len());
    for tool in tools {
        let object = tool
            .as_object()
            .ok_or_else(|| BridgeError::InvalidRequest("tool must be an object".into()))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BridgeError::InvalidRequest("tool name is missing".into()))?;
        let schema = object
            .get("input_schema")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                BridgeError::InvalidRequest("tool input_schema must be an object".into())
            })?;
        let mut function = Map::new();
        function.insert("name".into(), Value::String(name.to_string()));
        if let Some(description) = object.get("description").and_then(Value::as_str) {
            function.insert("description".into(), Value::String(description.to_string()));
        }
        function.insert("parameters".into(), Value::Object(schema.clone()));
        out.push(json!({"type": "function", "function": Value::Object(function)}));
    }
    Ok(Value::Array(out))
}

fn convert_tool_choice(tool_choice: &Value) -> Result<(Value, Option<bool>), BridgeError> {
    let object = tool_choice
        .as_object()
        .ok_or_else(|| BridgeError::InvalidRequest("tool_choice must be an object".into()))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::InvalidRequest("tool_choice type is missing".into()))?;
    let parallel = object
        .get("disable_parallel_tool_use")
        .and_then(Value::as_bool)
        .filter(|disabled| *disabled)
        .map(|_| false);
    let choice = match kind {
        "auto" => Value::String("auto".into()),
        "any" => Value::String("required".into()),
        "none" => Value::String("none".into()),
        "tool" => {
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| BridgeError::InvalidRequest("tool_choice name is missing".into()))?;
            json!({"type": "function", "function": {"name": name}})
        }
        other => {
            return Err(BridgeError::Unsupported(format!(
                "tool_choice type '{other}' is not supported"
            )));
        }
    };
    Ok((choice, parallel))
}

fn expect_block_type(object: &Map<String, Value>, role: &str) -> Result<(), BridgeError> {
    if object.get("type").and_then(Value::as_str).is_none() {
        return Err(BridgeError::InvalidRequest(format!(
            "{role} content block type is missing"
        )));
    }
    Ok(())
}

fn copy_number(source: &Map<String, Value>, key: &str, out: &mut Map<String, Value>) {
    if let Some(value) = source.get(key).and_then(Value::as_f64)
        && let Some(number) = serde_json::Number::from_f64(value)
    {
        out.insert(key.to_string(), Value::Number(number));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translate(value: Value) -> Value {
        let bytes = anthropic_to_openai(
            serde_json::to_string(&value).unwrap().as_bytes(),
            "public-model",
        )
        .expect("translation");
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn maps_system_messages_and_max_tokens() {
        let out = translate(json!({
            "model": "public-model",
            "max_tokens": 128,
            "stream": true,
            "system": [
                {"type": "text", "text": "first", "cache_control": {"type": "ephemeral"}},
                {"type": "text", "text": "second"}
            ],
            "messages": [{"role": "user", "content": "hello"}]
        }));
        assert_eq!(out["model"], "public-model");
        assert_eq!(out["max_tokens"], 128);
        assert_eq!(out["stream"], true);
        assert_eq!(out["stream_options"]["include_usage"], true);
        assert_eq!(out["messages"][0]["role"], "system");
        assert_eq!(out["messages"][0]["content"], "first\n\nsecond");
        assert_eq!(out["messages"][1]["content"], "hello");
        assert!(out.get("system").is_none());
    }

    #[test]
    fn maps_tools_and_tool_choice() {
        let out = translate(json!({
            "model": "public-model",
            "max_tokens": 64,
            "tools": [{
                "name": "Read",
                "description": "Read a file",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }],
            "tool_choice": {"type": "tool", "name": "Read", "disable_parallel_tool_use": true},
            "messages": [{"role": "user", "content": "read it"}]
        }));
        assert_eq!(out["tools"][0]["type"], "function");
        assert_eq!(out["tools"][0]["function"]["name"], "Read");
        assert_eq!(out["tools"][0]["function"]["parameters"]["type"], "object");
        assert_eq!(out["tool_choice"]["function"]["name"], "Read");
        assert_eq!(out["parallel_tool_calls"], false);
    }

    #[test]
    fn maps_tool_use_and_tool_result() {
        let out = translate(json!({
            "model": "public-model",
            "max_tokens": 64,
            "messages": [
                {"role": "user", "content": "read a.txt"},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Reading"},
                    {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"path": "a.txt"}},
                    {"type": "thinking", "thinking": "private"}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": "contents"}
                ]}
            ]
        }));
        assert_eq!(out["messages"][1]["role"], "assistant");
        assert_eq!(out["messages"][1]["content"], "Reading");
        assert_eq!(
            out["messages"][1]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"a.txt\"}"
        );
        assert_eq!(out["messages"][2]["role"], "tool");
        assert_eq!(out["messages"][2]["tool_call_id"], "toolu_1");
        assert_eq!(out["messages"][2]["content"], "contents");
        assert!(out["messages"][1].get("thinking").is_none());
    }

    #[test]
    fn maps_images_to_image_url() {
        let out = translate(json!({
            "model": "public-model",
            "max_tokens": 32,
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "what is this?"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
            ]}]
        }));
        assert_eq!(out["messages"][0]["content"][0]["type"], "text");
        assert_eq!(out["messages"][0]["content"][1]["type"], "image_url");
        assert_eq!(
            out["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn accepts_system_role_messages_from_agent_templates() {
        let out = translate(json!({
            "model": "public-model",
            "max_tokens": 32,
            "system": "top",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "system", "content": [{"type": "text", "text": "rules"}]}
            ]
        }));
        assert_eq!(out["messages"][0]["role"], "system");
        assert_eq!(out["messages"][0]["content"], "top\n\nrules");
        assert_eq!(out["messages"][1]["role"], "user");
        assert_eq!(out["messages"][1]["content"], "hi");
    }

    #[test]
    fn drops_anthropic_only_fields_and_rejects_missing_max_tokens() {
        let out = translate(json!({
            "model": "public-model",
            "max_tokens": 32,
            "temperature": 0.2,
            "top_k": 10,
            "thinking": {"type": "enabled", "budget_tokens": 1000},
            "messages": [{"role": "user", "content": "hi"}]
        }));
        assert!(out.get("top_k").is_none());
        assert!(out.get("thinking").is_none());
        assert_eq!(out["temperature"], 0.2);

        let err = anthropic_to_openai(
            br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#,
            "m",
        )
        .unwrap_err();
        assert!(matches!(err, BridgeError::InvalidRequest(_)));
    }
}
