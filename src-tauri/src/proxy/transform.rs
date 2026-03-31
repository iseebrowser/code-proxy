use serde_json::{json, Value};

pub fn openai_chat_to_anthropic(body: Value) -> Result<Value, String> {
    let mut result = json!({});

    if let Some(model) = body.get("model").and_then(|m| m.as_str()) {
        result["model"] = json!(model);
    }

    if let Some(max_tokens) = body.get("max_tokens") {
        result["max_tokens"] = max_tokens.clone();
    } else if let Some(max_completion_tokens) = body.get("max_completion_tokens") {
        result["max_tokens"] = max_completion_tokens.clone();
    }

    passthrough_field(&body, &mut result, "temperature");
    passthrough_field(&body, &mut result, "top_p");
    passthrough_field(&body, &mut result, "stream");

    if let Some(stop) = body.get("stop") {
        result["stop_sequences"] = match stop {
            Value::String(_) => json!([stop.clone()]),
            Value::Array(_) => stop.clone(),
            _ => Value::Null,
        };
    }

    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .filter_map(|tool| {
                let function = tool.get("function")?;
                Some(json!({
                    "name": function.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": function.get("description").cloned().unwrap_or(Value::Null),
                    "input_schema": clean_schema(function.get("parameters").cloned().unwrap_or(json!({})))
                }))
            })
            .collect();
        if !anthropic_tools.is_empty() {
            result["tools"] = json!(anthropic_tools);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_tool_choice_to_anthropic(tool_choice);
    }

    let mut system_parts = Vec::new();
    let mut messages = Vec::new();

    if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            if role == "system" {
                let blocks = openai_content_to_anthropic_blocks(msg.get("content"), false)?;
                system_parts.extend(blocks);
                continue;
            }

            if role == "tool" {
                let tool_use_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = openai_tool_content_to_string(msg.get("content"));
                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": content
                    }]
                }));
                continue;
            }

            let mut blocks = openai_content_to_anthropic_blocks(msg.get("content"), role == "assistant")?;
            if role == "assistant" {
                append_openai_tool_calls_to_anthropic(msg, &mut blocks)?;
            } else if msg.get("tool_calls").is_some() {
                return Err("tool_calls are only supported on assistant messages".to_string());
            }

            messages.push(json!({
                "role": role,
                "content": blocks
            }));
        }
    }

    result["messages"] = json!(messages);
    if !system_parts.is_empty() {
        result["system"] = json!(system_parts);
    }

    Ok(result)
}

pub fn anthropic_to_openai_chat(body: Value) -> Result<Value, String> {
    let mut result = json!({});

    if let Some(model) = body.get("model").and_then(|m| m.as_str()) {
        result["model"] = json!(model);
    }

    let mut messages = Vec::new();

    if let Some(system) = body.get("system") {
        if let Some(text) = system.as_str() {
            messages.push(json!({"role": "system", "content": text}));
        } else if let Some(parts) = system.as_array() {
            let blocks = anthropic_blocks_to_openai_content(parts, "system")?;
            messages.push(json!({"role": "system", "content": blocks}));
        }
    }

    if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = msg.get("content");
            let converted = anthropic_message_to_openai_messages(role, content)?;
            messages.extend(converted);
        }
    }

    result["messages"] = json!(messages);

    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("");
    if let Some(max_tokens) = body.get("max_tokens") {
        if is_openai_o_series(model) {
            result["max_completion_tokens"] = max_tokens.clone();
        } else {
            result["max_tokens"] = max_tokens.clone();
        }
    }

    passthrough_field(&body, &mut result, "temperature");
    passthrough_field(&body, &mut result, "top_p");
    passthrough_field(&body, &mut result, "stream");

    if let Some(stop_sequences) = body.get("stop_sequences") {
        result["stop"] = stop_sequences.clone();
    }

    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        let openai_tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "description": tool.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": clean_schema(tool.get("input_schema").cloned().unwrap_or(json!({})))
                    }
                })
            })
            .collect();
        if !openai_tools.is_empty() {
            result["tools"] = json!(openai_tools);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_tool_choice_to_openai(tool_choice);
    }

    Ok(result)
}

pub fn anthropic_to_openai_chat_response(body: Value) -> Result<Value, String> {
    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Anthropic response missing content".to_string())?;

    let mut text_parts = Vec::new();
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in content {
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(text.to_string());
                    content_parts.push(json!({"type": "text", "text": text}));
                }
            }
            "tool_use" => {
                let arguments = serde_json::to_string(
                    &block.get("input").cloned().unwrap_or(json!({})),
                )
                .unwrap_or_else(|_| "{}".to_string());
                tool_calls.push(json!({
                    "id": block.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    "type": "function",
                    "function": {
                        "name": block.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "arguments": arguments
                    }
                }));
            }
            "thinking" => {}
            _ => {}
        }
    }

    let message_content = if tool_calls.is_empty() && content_parts.len() <= 1 {
        content_parts
            .first()
            .and_then(|part| part.get("text"))
            .cloned()
            .unwrap_or(Value::Null)
    } else if content_parts.is_empty() {
        Value::Null
    } else {
        json!(content_parts)
    };

    let finish_reason = match body.get("stop_reason").and_then(|v| v.as_str()) {
        Some("tool_use") => "tool_calls",
        Some("max_tokens") => "length",
        _ => "stop",
    };

    let usage = anthropic_usage_to_openai_usage(body.get("usage"));

    Ok(json!({
        "id": body.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": body.get("model").and_then(|v| v.as_str()).unwrap_or(""),
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": message_content,
                "tool_calls": if tool_calls.is_empty() { Value::Null } else { json!(tool_calls) }
            },
            "finish_reason": finish_reason
        }],
        "usage": usage
    }))
}

pub fn openai_chat_to_anthropic_response(body: Value) -> Result<Value, String> {
    let choices = body
        .get("choices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "OpenAI response missing choices".to_string())?;
    let choice = choices
        .first()
        .ok_or_else(|| "OpenAI response choices empty".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "OpenAI response missing message".to_string())?;

    let mut content = Vec::new();

    if let Some(msg_content) = message.get("content") {
        if let Some(text) = msg_content.as_str() {
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
        } else if let Some(parts) = msg_content.as_array() {
            for part in parts {
                match part.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "text" | "output_text" => {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                content.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    "refusal" => {
                        if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                content.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tool_call in tool_calls {
            let arguments = tool_call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": tool_call.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "name": tool_call.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or(""),
                "input": input
            }));
        }
    }

    if let Some(function_call) = message.get("function_call") {
        let arguments = function_call
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let input = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
        content.push(json!({
            "type": "tool_use",
            "id": function_call.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "name": function_call.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "input": input
        }));
    }

    let stop_reason = match choice.get("finish_reason").and_then(|v| v.as_str()) {
        Some("tool_calls") | Some("function_call") => Some("tool_use"),
        Some("length") => Some("max_tokens"),
        Some("stop") | Some("content_filter") => Some("end_turn"),
        Some(_) | None => Some("end_turn"),
    };

    Ok(json!({
        "id": body.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": body.get("model").and_then(|v| v.as_str()).unwrap_or(""),
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": openai_usage_to_anthropic_usage(body.get("usage"))
    }))
}

pub fn is_openai_o_series(model: &str) -> bool {
    model.len() > 1
        && model.starts_with('o')
        && model.as_bytes().get(1).is_some_and(|b| b.is_ascii_digit())
}

pub fn clean_schema(mut schema: Value) -> Value {
    if let Some(obj) = schema.as_object_mut() {
        if obj.get("format").and_then(|v| v.as_str()) == Some("uri") {
            obj.remove("format");
        }
        if let Some(properties) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
            for value in properties.values_mut() {
                *value = clean_schema(value.clone());
            }
        }
        if let Some(items) = obj.get_mut("items") {
            *items = clean_schema(items.clone());
        }
    }
    schema
}

fn passthrough_field(source: &Value, target: &mut Value, key: &str) {
    if let Some(value) = source.get(key) {
        target[key] = value.clone();
    }
}

fn map_tool_choice_to_anthropic(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::String(s) => match s.as_str() {
            "required" => json!({"type": "any"}),
            "auto" => json!({"type": "auto"}),
            "none" => json!({"type": "none"}),
            _ => tool_choice.clone(),
        },
        Value::Object(obj) => match obj.get("type").and_then(|v| v.as_str()) {
            Some("function") => json!({
                "type": "tool",
                "name": obj.get("function")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    .or_else(|| obj.get("name").and_then(|v| v.as_str()))
                    .unwrap_or("")
            }),
            _ => tool_choice.clone(),
        },
        _ => tool_choice.clone(),
    }
}

fn map_tool_choice_to_openai(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::Object(obj) => match obj.get("type").and_then(|v| v.as_str()) {
            Some("any") => json!("required"),
            Some("auto") => json!("auto"),
            Some("none") => json!("none"),
            Some("tool") => json!({
                "type": "function",
                "function": {
                    "name": obj.get("name").and_then(|v| v.as_str()).unwrap_or("")
                }
            }),
            _ => tool_choice.clone(),
        },
        _ => tool_choice.clone(),
    }
}

fn openai_content_to_anthropic_blocks(
    content: Option<&Value>,
    allow_assistant_images: bool,
) -> Result<Vec<Value>, String> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };

    if let Some(text) = content.as_str() {
        return Ok(vec![json!({"type": "text", "text": text})]);
    }

    if let Some(parts) = content.as_array() {
        let mut blocks = Vec::new();
        for part in parts {
            match part.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "text" | "input_text" | "output_text" => {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        blocks.push(json!({"type": "text", "text": text}));
                    }
                }
                "image_url" | "input_image" => {
                    if !allow_assistant_images {
                        let url = part
                            .get("image_url")
                            .and_then(|v| v.get("url"))
                            .and_then(|v| v.as_str())
                            .or_else(|| part.get("image_url").and_then(|v| v.as_str()));
                        if let Some(url) = url {
                            if let Some((media_type, data)) = parse_data_url(url) {
                                blocks.push(json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": data
                                    }
                                }));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        return Ok(blocks);
    }

    Ok(Vec::new())
}

fn append_openai_tool_calls_to_anthropic(message: &Value, blocks: &mut Vec<Value>) -> Result<(), String> {
    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tool_call in tool_calls {
            let arguments = tool_call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input = serde_json::from_str::<Value>(arguments)
                .map_err(|e| format!("Invalid tool call arguments: {e}"))?;
            blocks.push(json!({
                "type": "tool_use",
                "id": tool_call.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "name": tool_call.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or(""),
                "input": input
            }));
        }
    } else if let Some(function_call) = message.get("function_call") {
        let arguments = function_call
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");
        let input = serde_json::from_str::<Value>(arguments)
            .map_err(|e| format!("Invalid function_call arguments: {e}"))?;
        blocks.push(json!({
            "type": "tool_use",
            "id": function_call.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "name": function_call.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "input": input
        }));
    }
    Ok(())
}

fn anthropic_message_to_openai_messages(role: &str, content: Option<&Value>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    let Some(content) = content else {
        result.push(json!({"role": role, "content": Value::Null}));
        return Ok(result);
    };

    if let Some(text) = content.as_str() {
        result.push(json!({"role": role, "content": text}));
        return Ok(result);
    }

    let Some(blocks) = content.as_array() else {
        result.push(json!({"role": role, "content": content.clone()}));
        return Ok(result);
    };

    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "text" => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    content_parts.push(json!({"type": "text", "text": text}));
                }
            }
            "image" => {
                if let Some(source) = block.get("source") {
                    let media_type = source
                        .get("media_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("image/png");
                    let data = source.get("data").and_then(|v| v.as_str()).unwrap_or("");
                    content_parts.push(json!({
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:{media_type};base64,{data}")
                        }
                    }));
                }
            }
            "tool_use" => {
                tool_calls.push(json!({
                    "id": block.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    "type": "function",
                    "function": {
                        "name": block.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "arguments": serde_json::to_string(&block.get("input").cloned().unwrap_or(json!({})))
                            .unwrap_or_else(|_| "{}".to_string())
                    }
                }));
            }
            "tool_result" => {
                result.push(json!({
                    "role": "tool",
                    "tool_call_id": block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": openai_tool_content_to_string(block.get("content"))
                }));
            }
            "thinking" => {}
            _ => {}
        }
    }

    if !content_parts.is_empty() || !tool_calls.is_empty() {
        let content_value = if content_parts.is_empty() {
            Value::Null
        } else if content_parts.len() == 1 && content_parts[0].get("type") == Some(&json!("text")) {
            content_parts[0]["text"].clone()
        } else {
            json!(content_parts)
        };

        let mut message = json!({
            "role": role,
            "content": content_value
        });
        if !tool_calls.is_empty() {
            message["tool_calls"] = json!(tool_calls);
        }
        result.push(message);
    }

    Ok(result)
}

fn anthropic_blocks_to_openai_content(blocks: &[Value], role: &str) -> Result<Value, String> {
    let messages = anthropic_message_to_openai_messages(role, Some(&json!(blocks)))?;
    Ok(messages
        .first()
        .and_then(|msg| msg.get("content"))
        .cloned()
        .unwrap_or(Value::Null))
}

fn openai_tool_content_to_string(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(value) => serde_json::to_string(value).unwrap_or_default(),
        None => String::new(),
    }
}

fn parse_data_url(url: &str) -> Option<(String, String)> {
    let without_prefix = url.strip_prefix("data:")?;
    let (metadata, data) = without_prefix.split_once(',')?;
    let media_type = metadata.strip_suffix(";base64").unwrap_or(metadata);
    Some((media_type.to_string(), data.to_string()))
}

fn anthropic_usage_to_openai_usage(usage: Option<&Value>) -> Value {
    let usage = usage.cloned().unwrap_or_else(|| json!({}));
    let prompt_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut result = json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": prompt_tokens + completion_tokens
    });

    if let Some(cached) = usage.get("cache_read_input_tokens") {
        result["prompt_tokens_details"] = json!({
            "cached_tokens": cached
        });
    }

    result
}

fn openai_usage_to_anthropic_usage(usage: Option<&Value>) -> Value {
    let usage = usage.cloned().unwrap_or_else(|| json!({}));
    let mut result = json!({
        "input_tokens": usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        "output_tokens": usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
    });

    if let Some(cached) = usage.pointer("/prompt_tokens_details/cached_tokens") {
        result["cache_read_input_tokens"] = cached.clone();
    }
    if let Some(cached) = usage.get("cache_read_input_tokens") {
        result["cache_read_input_tokens"] = cached.clone();
    }
    if let Some(created) = usage.get("cache_creation_input_tokens") {
        result["cache_creation_input_tokens"] = created.clone();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_chat_request_maps_system_tool_and_user_message() {
        let input = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "Hello"},
                {
                    "role": "assistant",
                    "content": "Calling tool",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"hello\"}"}
                    }]
                },
                {"role": "tool", "tool_call_id": "call_1", "content": "world"}
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "lookup",
                    "description": "Lookup",
                    "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
                }
            }],
            "tool_choice": "required",
            "max_tokens": 1234
        });

        let result = openai_chat_to_anthropic(input).unwrap();
        assert_eq!(result["system"][0]["text"], "You are helpful");
        assert_eq!(result["messages"][0]["role"], "user");
        assert_eq!(result["messages"][0]["content"][0]["text"], "Hello");
        assert_eq!(result["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(result["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(result["tools"][0]["name"], "lookup");
        assert_eq!(result["tool_choice"]["type"], "any");
        assert_eq!(result["max_tokens"], 1234);
    }

    #[test]
    fn anthropic_request_maps_to_openai_chat_messages() {
        let input = json!({
            "model": "gpt-4o",
            "system": [{"type": "text", "text": "You are helpful"}],
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Checking"},
                    {"type": "tool_use", "id": "call_1", "name": "lookup", "input": {"q": "hello"}}
                ]
            }, {
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "call_1", "content": "world"}
                ]
            }],
            "tools": [{
                "name": "lookup",
                "description": "Lookup",
                "input_schema": {"type": "object"}
            }],
            "tool_choice": {"type": "tool", "name": "lookup"},
            "max_tokens": 100
        });

        let result = anthropic_to_openai_chat(input).unwrap();
        assert_eq!(result["messages"][0]["role"], "system");
        assert_eq!(result["messages"][1]["tool_calls"][0]["function"]["name"], "lookup");
        assert_eq!(result["messages"][2]["role"], "tool");
        assert_eq!(result["messages"][2]["tool_call_id"], "call_1");
        assert_eq!(result["tools"][0]["function"]["name"], "lookup");
        assert_eq!(result["tool_choice"]["function"]["name"], "lookup");
        assert_eq!(result["max_tokens"], 100);
    }

    #[test]
    fn anthropic_response_maps_to_openai_chat_response() {
        let input = json!({
            "id": "msg_1",
            "model": "claude-test",
            "stop_reason": "tool_use",
            "content": [
                {"type": "text", "text": "Checking"},
                {"type": "tool_use", "id": "call_1", "name": "lookup", "input": {"q": "hello"}}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 4}
        });

        let result = anthropic_to_openai_chat_response(input).unwrap();
        assert_eq!(result["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "lookup");
        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(result["usage"]["prompt_tokens"], 10);
        assert_eq!(result["usage"]["prompt_tokens_details"]["cached_tokens"], 4);
    }

    #[test]
    fn openai_chat_response_maps_to_anthropic_response() {
        let input = json!({
            "id": "chatcmpl_1",
            "model": "gpt-4o",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Done",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"hello\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        });

        let result = openai_chat_to_anthropic_response(input).unwrap();
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][1]["type"], "tool_use");
        assert_eq!(result["stop_reason"], "tool_use");
        assert_eq!(result["usage"]["input_tokens"], 10);
    }
}
