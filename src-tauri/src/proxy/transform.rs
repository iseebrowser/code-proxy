use serde_json::{json, Value};

/// Transform OpenAI chat completion request to Anthropic messages request
pub fn transform_openai_to_anthropic(body: &str) -> Result<String, String> {
    let oai: Value = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse OpenAI request: {}", e))?;

    let model = oai.get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("claude-3-5-sonnet-20241022");

    // Map common models
    let anthropic_model = map_to_anthropic_model(model);

    let messages = oai.get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter().filter_map(|msg| {
                let role = msg.get("role")?.as_str()?;
                let content = msg.get("content")?.as_str()?;

                // Skip system messages in this simple transform
                if role == "system" {
                    return None;
                }

                Some(json!({
                    "type": "text",
                    "text": content
                }))
            }).collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let max_tokens = oai.get("max_tokens")
        .and_then(|t| t.as_i64())
        .unwrap_or(4096);

    let temperature = oai.get("temperature")
        .and_then(|t| t.as_f64());

    let result = json!({
        "model": anthropic_model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": temperature
    });

    serde_json::to_string(&result)
        .map_err(|e| format!("Failed to serialize Anthropic request: {}", e))
}

/// Transform Anthropic messages request to OpenAI chat completion request
pub fn transform_anthropic_to_openai(body: &str) -> Result<String, String> {
    let anthropic: Value = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse Anthropic request: {}", e))?;

    let model = anthropic.get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("gpt-4o");

    // Map common models
    let openai_model = map_to_openai_model(model);

    let messages = anthropic.get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter().filter_map(|msg| {
                let role = msg.get("role")?.as_str()?;
                let content = msg.get("content")?;

                let text = content.get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                Some(json!({
                    "role": if role == "user" { "user" } else { "assistant" },
                    "content": text
                }))
            }).collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let max_tokens = anthropic.get("max_tokens")
        .and_then(|t| t.as_i64())
        .unwrap_or(4096);

    let temperature = anthropic.get("temperature")
        .and_then(|t| t.as_f64());

    let result = json!({
        "model": openai_model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": temperature
    });

    serde_json::to_string(&result)
        .map_err(|e| format!("Failed to serialize OpenAI request: {}", e))
}

/// Map OpenAI model to Anthropic model
fn map_to_anthropic_model(model: &str) -> String {
    let model_lower = model.to_lowercase();
    if model_lower.contains("opus") {
        "claude-opus-4-5-20251101".to_string()
    } else if model_lower.contains("sonnet") {
        "claude-3-5-sonnet-20241022".to_string()
    } else if model_lower.contains("haiku") {
        "claude-3-5-haiku-20241022".to_string()
    } else if model_lower.contains("claude") {
        model.to_string()
    } else {
        // Default to sonnet for unknown models
        "claude-3-5-sonnet-20241022".to_string()
    }
}

/// Map Anthropic model to OpenAI model
fn map_to_openai_model(model: &str) -> String {
    let model_lower = model.to_lowercase();
    if model_lower.contains("opus") {
        "gpt-4".to_string()
    } else if model_lower.contains("sonnet") || model_lower.contains("claude-3-5") {
        "gpt-4o".to_string()
    } else if model_lower.contains("haiku") {
        "gpt-4o-mini".to_string()
    } else if model_lower.contains("gpt") {
        model.to_string()
    } else {
        // Default to gpt-4o for unknown models
        "gpt-4o".to_string()
    }
}
