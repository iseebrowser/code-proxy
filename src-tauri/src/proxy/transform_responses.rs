use serde_json::{json, Value};

use super::transform::clean_schema;

pub fn anthropic_to_openai_responses(body: Value) -> Result<Value, String> {
    let mut result = json!({});

    if let Some(model) = body.get("model").and_then(|v| v.as_str()) {
        result["model"] = json!(model);
    }
    if let Some(system) = body.get("system") {
        let instructions = if let Some(text) = system.as_str() {
            text.to_string()
        } else if let Some(parts) = system.as_array() {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join("\n\n")
        } else {
            String::new()
        };
        if !instructions.is_empty() {
            result["instructions"] = json!(instructions);
        }
    }

    if let Some(max_tokens) = body.get("max_tokens") {
        result["max_output_tokens"] = max_tokens.clone();
    }

    passthrough_field(&body, &mut result, "temperature");
    passthrough_field(&body, &mut result, "top_p");
    passthrough_field(&body, &mut result, "stream");

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        result["input"] = json!(anthropic_messages_to_responses_input(messages)?);
    }

    if let Some(tools) = body.get("tools").and_then(|v| v.as_array()) {
        let response_tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": clean_schema(tool.get("input_schema").cloned().unwrap_or(json!({})))
                })
            })
            .collect();
        if !response_tools.is_empty() {
            result["tools"] = json!(response_tools);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_tool_choice_to_responses(tool_choice);
    }

    Ok(result)
}

pub fn openai_responses_to_anthropic(body: Value) -> Result<Value, String> {
    let mut result = json!({
        "messages": []
    });

    if let Some(model) = body.get("model").and_then(|v| v.as_str()) {
        result["model"] = json!(model);
    }
    if let Some(max_tokens) = body.get("max_output_tokens").or_else(|| body.get("max_tokens")) {
        result["max_tokens"] = max_tokens.clone();
    }
    if let Some(instructions) = body.get("instructions").and_then(|v| v.as_str()) {
        result["system"] = json!([{ "type": "text", "text": instructions }]);
    }

    passthrough_field(&body, &mut result, "temperature");
    passthrough_field(&body, &mut result, "top_p");
    passthrough_field(&body, &mut result, "stream");

    if let Some(tools) = body.get("tools").and_then(|v| v.as_array()) {
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "input_schema": clean_schema(tool.get("parameters").cloned().unwrap_or(json!({})))
                })
            })
            .collect();
        if !anthropic_tools.is_empty() {
            result["tools"] = json!(anthropic_tools);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = map_tool_choice_from_responses(tool_choice);
    }

    let mut messages = Vec::new();
    if let Some(input) = body.get("input").and_then(|v| v.as_array()) {
        for item in input {
            if let Some(role) = item.get("role").and_then(|v| v.as_str()) {
                let content = responses_content_to_anthropic_blocks(item.get("content"))?;
                messages.push(json!({
                    "role": role,
                    "content": content
                }));
                continue;
            }

            match item.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "function_call" => {
                    let arguments = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                    let input = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
                    messages.push(json!({
                        "role": "assistant",
                        "content": [{
                            "type": "tool_use",
                            "id": item.get("call_id").and_then(|v| v.as_str()).unwrap_or(""),
                            "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                            "input": input
                        }]
                    }));
                }
                "function_call_output" => {
                    messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": item.get("call_id").and_then(|v| v.as_str()).unwrap_or(""),
                            "content": item.get("output").cloned().unwrap_or_else(|| json!(""))
                        }]
                    }));
                }
                _ => {}
            }
        }
    }

    result["messages"] = json!(messages);
    Ok(result)
}

pub fn anthropic_to_openai_responses_response(body: Value) -> Result<Value, String> {
    let content = body
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Anthropic response missing content".to_string())?;

    let mut output = Vec::new();
    let mut message_content = Vec::new();

    for block in content {
        match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "text" => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    message_content.push(json!({"type": "output_text", "text": text}));
                }
            }
            "tool_use" => {
                if !message_content.is_empty() {
                    output.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": message_content.clone()
                    }));
                    message_content.clear();
                }
                output.push(json!({
                    "type": "function_call",
                    "call_id": block.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                    "name": block.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "arguments": serde_json::to_string(&block.get("input").cloned().unwrap_or(json!({})))
                        .unwrap_or_else(|_| "{}".to_string()),
                    "status": "completed"
                }));
            }
            "thinking" => {
                if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                    output.push(json!({
                        "type": "reasoning",
                        "summary": [{
                            "type": "summary_text",
                            "text": thinking
                        }]
                    }));
                }
            }
            _ => {}
        }
    }

    if !message_content.is_empty() {
        output.push(json!({
            "type": "message",
            "role": "assistant",
            "content": message_content
        }));
    }

    let stop_reason = body.get("stop_reason").and_then(|v| v.as_str());
    let (status, incomplete_details) = match stop_reason {
        Some("max_tokens") => ("incomplete", json!({"reason": "max_output_tokens"})),
        _ => ("completed", Value::Null),
    };

    Ok(json!({
        "id": body.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "object": "response",
        "status": status,
        "incomplete_details": incomplete_details,
        "model": body.get("model").and_then(|v| v.as_str()).unwrap_or(""),
        "output": output,
        "usage": anthropic_usage_to_responses_usage(body.get("usage"))
    }))
}

pub fn openai_responses_to_anthropic_response(body: Value) -> Result<Value, String> {
    let output = body
        .get("output")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Responses API response missing output".to_string())?;

    let mut content = Vec::new();
    let mut has_tool_use = false;

    for item in output {
        match item.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "message" => {
                if let Some(parts) = item.get("content").and_then(|v| v.as_array()) {
                    for part in parts {
                        match part.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                            "output_text" => {
                                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                            "refusal" => {
                                if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "function_call" => {
                has_tool_use = true;
                let arguments = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                let input = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
                content.push(json!({
                    "type": "tool_use",
                    "id": item.get("call_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "input": input
                }));
            }
            "reasoning" => {
                if let Some(summary) = item.get("summary").and_then(|v| v.as_array()) {
                    let text = summary
                        .iter()
                        .filter_map(|part| part.get("text").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        content.push(json!({"type": "thinking", "thinking": text}));
                    }
                }
            }
            _ => {}
        }
    }

    let stop_reason = match body.get("status").and_then(|v| v.as_str()) {
        Some("incomplete") => Some("max_tokens"),
        Some("completed") => {
            if has_tool_use {
                Some("tool_use")
            } else {
                Some("end_turn")
            }
        }
        _ => Some("end_turn"),
    };

    Ok(json!({
        "id": body.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": body.get("model").and_then(|v| v.as_str()).unwrap_or(""),
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": responses_usage_to_anthropic_usage(body.get("usage"))
    }))
}

fn anthropic_messages_to_responses_input(messages: &[Value]) -> Result<Vec<Value>, String> {
    let mut input = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let content = msg.get("content");
        if let Some(text) = content.and_then(|v| v.as_str()) {
            input.push(json!({
                "role": role,
                "content": [{
                    "type": if role == "assistant" { "output_text" } else { "input_text" },
                    "text": text
                }]
            }));
            continue;
        }

        let Some(parts) = content.and_then(|v| v.as_array()) else {
            input.push(json!({"role": role}));
            continue;
        };

        let mut message_content = Vec::new();
        for part in parts {
            match part.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "text" => {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        message_content.push(json!({
                            "type": if role == "assistant" { "output_text" } else { "input_text" },
                            "text": text
                        }));
                    }
                }
                "image" => {
                    if let Some(source) = part.get("source") {
                        let media_type = source.get("media_type").and_then(|v| v.as_str()).unwrap_or("image/png");
                        let data = source.get("data").and_then(|v| v.as_str()).unwrap_or("");
                        message_content.push(json!({
                            "type": "input_image",
                            "image_url": format!("data:{media_type};base64,{data}")
                        }));
                    }
                }
                "tool_use" => {
                    if !message_content.is_empty() {
                        input.push(json!({"role": role, "content": message_content.clone()}));
                        message_content.clear();
                    }
                    input.push(json!({
                        "type": "function_call",
                        "call_id": part.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        "name": part.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "arguments": serde_json::to_string(&part.get("input").cloned().unwrap_or(json!({})))
                            .unwrap_or_else(|_| "{}".to_string())
                    }));
                }
                "tool_result" => {
                    if !message_content.is_empty() {
                        input.push(json!({"role": role, "content": message_content.clone()}));
                        message_content.clear();
                    }
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": part.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or(""),
                        "output": part.get("content").cloned().unwrap_or_else(|| json!(""))
                    }));
                }
                "thinking" => {}
                _ => {}
            }
        }

        if !message_content.is_empty() {
            input.push(json!({"role": role, "content": message_content}));
        }
    }

    Ok(input)
}

fn responses_content_to_anthropic_blocks(content: Option<&Value>) -> Result<Vec<Value>, String> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };

    let Some(parts) = content.as_array() else {
        return Ok(Vec::new());
    };

    let mut blocks = Vec::new();
    for part in parts {
        match part.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "input_text" | "output_text" => {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    blocks.push(json!({"type": "text", "text": text}));
                }
            }
            "input_image" => {
                let url = part.get("image_url").and_then(|v| v.as_str()).unwrap_or("");
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
            _ => {}
        }
    }

    Ok(blocks)
}

fn map_tool_choice_to_responses(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::Object(obj) => match obj.get("type").and_then(|v| v.as_str()) {
            Some("any") => json!("required"),
            Some("auto") => json!("auto"),
            Some("none") => json!("none"),
            Some("tool") => json!({
                "type": "function",
                "name": obj.get("name").and_then(|v| v.as_str()).unwrap_or("")
            }),
            _ => tool_choice.clone(),
        },
        _ => tool_choice.clone(),
    }
}

fn map_tool_choice_from_responses(tool_choice: &Value) -> Value {
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
                "name": obj.get("name").and_then(|v| v.as_str()).unwrap_or("")
            }),
            _ => tool_choice.clone(),
        },
        _ => tool_choice.clone(),
    }
}

fn anthropic_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let usage = usage.cloned().unwrap_or_else(|| json!({}));
    let mut result = json!({
        "input_tokens": usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        "output_tokens": usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
    });
    if let Some(cached) = usage.get("cache_read_input_tokens") {
        result["input_tokens_details"] = json!({"cached_tokens": cached});
    }
    result
}

fn responses_usage_to_anthropic_usage(usage: Option<&Value>) -> Value {
    let usage = usage.cloned().unwrap_or_else(|| json!({}));
    let mut result = json!({
        "input_tokens": usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        "output_tokens": usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
    });
    if let Some(cached) = usage.pointer("/input_tokens_details/cached_tokens") {
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

fn parse_data_url(url: &str) -> Option<(String, String)> {
    let without_prefix = url.strip_prefix("data:")?;
    let (metadata, data) = without_prefix.split_once(',')?;
    let media_type = metadata.strip_suffix(";base64").unwrap_or(metadata);
    Some((media_type.to_string(), data.to_string()))
}

fn passthrough_field(source: &Value, target: &mut Value, key: &str) {
    if let Some(value) = source.get(key) {
        target[key] = value.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_request_maps_to_openai_responses() {
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
            }]
        });

        let result = anthropic_to_openai_responses(input).unwrap();
        assert_eq!(result["instructions"], "You are helpful");
        assert_eq!(result["input"][0]["role"], "assistant");
        assert_eq!(result["input"][1]["type"], "function_call");
        assert_eq!(result["input"][2]["type"], "function_call_output");
    }

    #[test]
    fn openai_responses_request_maps_to_anthropic() {
        let input = json!({
            "model": "gpt-4o",
            "instructions": "You are helpful",
            "max_output_tokens": 100,
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "Hello"}]},
                {"type": "function_call_output", "call_id": "call_1", "output": "world"}
            ]
        });

        let result = openai_responses_to_anthropic(input).unwrap();
        assert_eq!(result["system"][0]["text"], "You are helpful");
        assert_eq!(result["messages"][0]["content"][0]["text"], "Hello");
        assert_eq!(result["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(result["max_tokens"], 100);
    }

    #[test]
    fn anthropic_response_maps_to_openai_responses_response() {
        let input = json!({
            "id": "msg_1",
            "model": "claude-test",
            "stop_reason": "tool_use",
            "content": [
                {"type": "thinking", "thinking": "Need lookup"},
                {"type": "text", "text": "Checking"},
                {"type": "tool_use", "id": "call_1", "name": "lookup", "input": {"q": "hello"}}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 4}
        });

        let result = anthropic_to_openai_responses_response(input).unwrap();
        assert_eq!(result["output"][0]["type"], "reasoning");
        assert_eq!(result["output"][1]["type"], "message");
        assert_eq!(result["output"][2]["type"], "function_call");
        assert_eq!(result["usage"]["input_tokens_details"]["cached_tokens"], 4);
    }

    #[test]
    fn openai_responses_response_maps_to_anthropic_response() {
        let input = json!({
            "id": "resp_1",
            "status": "completed",
            "model": "gpt-4o",
            "output": [
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "Need lookup"}]},
                {"type": "message", "content": [{"type": "output_text", "text": "Checking"}]},
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"hello\"}"}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });

        let result = openai_responses_to_anthropic_response(input).unwrap();
        assert_eq!(result["content"][0]["type"], "thinking");
        assert_eq!(result["content"][1]["type"], "text");
        assert_eq!(result["content"][2]["type"], "tool_use");
        assert_eq!(result["stop_reason"], "tool_use");
    }
}
