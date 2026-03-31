use std::{collections::{HashMap, HashSet}, pin::Pin};

use async_stream::stream;
use axum::{body::{Body, Bytes}, response::Response};
use futures::{Stream, StreamExt};
use http::HeaderValue;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug)]
pub enum StreamRewrite {
    Passthrough,
    AnthropicToOpenAiChat,
    AnthropicToOpenAiResponses,
    OpenAiChatToAnthropic,
    OpenAiResponsesToAnthropic,
}

pub fn is_stream_requested(body: &Value) -> bool {
    body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn is_sse_content_type(value: Option<&HeaderValue>) -> bool {
    value
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("text/event-stream"))
}

pub fn streaming_response_from_reqwest(
    upstream: reqwest::Response,
    rewrite: StreamRewrite,
) -> Response<Body> {
    let status = upstream.status();
    let request_id = upstream.headers().get("x-request-id").cloned();
    let trace_id = upstream.headers().get("x-trace-id").cloned();
    let stream = upstream.bytes_stream().map(|item| item.map_err(std::io::Error::other));

    let out_stream: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> = match rewrite {
        StreamRewrite::Passthrough => Box::pin(stream),
        other => Box::pin(rewrite_sse_stream(stream, other)),
    };

    let mut builder = Response::builder()
        .status(status)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive");

    if let Some(value) = request_id {
        builder = builder.header("x-request-id", value);
    }
    if let Some(value) = trace_id {
        builder = builder.header("x-trace-id", value);
    }

    builder.body(Body::from_stream(out_stream)).unwrap()
}

pub fn anthropic_sse_to_openai_chat(input: &str) -> Result<String, String> {
    let mut state = AnthropicToOpenAiChatState::default();
    let mut out = String::new();
    for block in parse_sse_blocks(input) {
        out.push_str(&state.handle(block)?);
    }
    Ok(out)
}

pub fn openai_chat_sse_to_anthropic(input: &str) -> Result<String, String> {
    let mut state = OpenAiChatToAnthropicState::default();
    let mut out = String::new();
    for block in parse_sse_blocks(input) {
        out.push_str(&state.handle(block)?);
    }
    Ok(out)
}

pub fn anthropic_sse_to_openai_responses(input: &str) -> Result<String, String> {
    let mut state = AnthropicToOpenAiResponsesState::default();
    let mut out = String::new();
    for block in parse_sse_blocks(input) {
        out.push_str(&state.handle(block)?);
    }
    Ok(out)
}

pub fn openai_responses_sse_to_anthropic(input: &str) -> Result<String, String> {
    let mut state = OpenAiResponsesToAnthropicState::default();
    let mut out = String::new();
    for block in parse_sse_blocks(input) {
        out.push_str(&state.handle(block)?);
    }
    Ok(out)
}

fn rewrite_sse_stream<E>(
    input: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
    rewrite: StreamRewrite,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send
where
    E: std::error::Error + Send + 'static,
{
    stream! {
        let mut buffer = String::new();
        tokio::pin!(input);

        while let Some(chunk) = input.next().await {
            match chunk {
                Ok(bytes) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(pos) = buffer.find("\n\n") {
                        let block = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();
                        if block.trim().is_empty() {
                            continue;
                        }
                        let rewritten = match rewrite {
                            StreamRewrite::Passthrough => Ok(format!("{block}\n\n")),
                            StreamRewrite::AnthropicToOpenAiChat => anthropic_sse_to_openai_chat(&(block.clone() + "\n\n")),
                            StreamRewrite::AnthropicToOpenAiResponses => anthropic_sse_to_openai_responses(&(block.clone() + "\n\n")),
                            StreamRewrite::OpenAiChatToAnthropic => openai_chat_sse_to_anthropic(&(block.clone() + "\n\n")),
                            StreamRewrite::OpenAiResponsesToAnthropic => openai_responses_sse_to_anthropic(&(block.clone() + "\n\n")),
                        };
                        match rewritten {
                            Ok(text) if !text.is_empty() => yield Ok(Bytes::from(text)),
                            Ok(_) => {}
                            Err(error) => {
                                yield Ok(Bytes::from(sse_error_event(&error)));
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    yield Ok(Bytes::from(sse_error_event(&format!("Stream error: {error}"))));
                    break;
                }
            }
        }
    }
}

fn sse_error_event(message: &str) -> String {
    let event = json!({
        "type": "error",
        "error": {
            "type": "stream_error",
            "message": message
        }
    });
    format!("event: error\ndata: {}\n\n", serde_json::to_string(&event).unwrap_or_default())
}

#[derive(Debug, Clone)]
struct SseBlock {
    event: Option<String>,
    data: Vec<String>,
}

fn parse_sse_blocks(input: &str) -> Vec<SseBlock> {
    input
        .split("\n\n")
        .filter_map(|block| {
            if block.trim().is_empty() {
                return None;
            }
            let mut event = None;
            let mut data = Vec::new();
            for line in block.lines() {
                if let Some(value) = strip_sse_field(line, "event") {
                    event = Some(value.trim().to_string());
                } else if let Some(value) = strip_sse_field(line, "data") {
                    data.push(value.to_string());
                }
            }
            if data.is_empty() {
                None
            } else {
                Some(SseBlock { event, data })
            }
        })
        .collect()
}

fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.strip_prefix(field)
        .and_then(|rest| rest.strip_prefix(':'))
        .map(|v| v.trim_start())
}

fn to_json_lines(data: &[String]) -> Result<Value, String> {
    serde_json::from_str(&data.join("\n")).map_err(|e| format!("Invalid SSE JSON payload: {e}"))
}

fn sse_data_line(value: &Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(value).unwrap_or_default())
}

fn sse_named_event(name: &str, value: &Value) -> String {
    format!("event: {name}\ndata: {}\n\n", serde_json::to_string(value).unwrap_or_default())
}

#[derive(Default, Clone)]
struct ToolCallState {
    id: String,
    name: String,
}

#[derive(Default)]
struct AnthropicToOpenAiChatState {
    message_id: String,
    model: String,
    tool_indices: HashMap<u64, ToolCallState>,
    usage: Option<Value>,
}

impl AnthropicToOpenAiChatState {
    fn handle(&mut self, block: SseBlock) -> Result<String, String> {
        let data = if block.data.len() == 1 && block.data[0].trim() == "[DONE]" {
            return Ok(String::new());
        } else {
            to_json_lines(&block.data)?
        };

        match data.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "message_start" => {
                self.message_id = data
                    .pointer("/message/id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("chatcmpl_proxy")
                    .to_string();
                self.model = data
                    .pointer("/message/model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(sse_data_line(&json!({
                    "id": self.message_id,
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": self.model,
                    "choices": [{
                        "index": 0,
                        "delta": { "role": "assistant" },
                        "finish_reason": Value::Null
                    }]
                })))
            }
            "content_block_start" => {
                let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let block_type = data
                    .pointer("/content_block/type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match block_type {
                    "tool_use" => {
                        let id = data
                            .pointer("/content_block/id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = data
                            .pointer("/content_block/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        self.tool_indices
                            .insert(index, ToolCallState { id: id.clone(), name: name.clone() });
                        Ok(sse_data_line(&json!({
                            "id": self.message_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": self.model,
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_calls": [{
                                        "index": index,
                                        "id": id,
                                        "type": "function",
                                        "function": { "name": name }
                                    }]
                                },
                                "finish_reason": Value::Null
                            }]
                        })))
                    }
                    _ => Ok(String::new()),
                }
            }
            "content_block_delta" => {
                let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let delta_type = data
                    .pointer("/delta/type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match delta_type {
                    "text_delta" => {
                        let text = data
                            .pointer("/delta/text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        Ok(sse_data_line(&json!({
                            "id": self.message_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": self.model,
                            "choices": [{
                                "index": 0,
                                "delta": { "content": text },
                                "finish_reason": Value::Null
                            }]
                        })))
                    }
                    "thinking_delta" => {
                        let thinking = data
                            .pointer("/delta/thinking")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        Ok(sse_data_line(&json!({
                            "id": self.message_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": self.model,
                            "choices": [{
                                "index": 0,
                                "delta": { "reasoning": thinking },
                                "finish_reason": Value::Null
                            }]
                        })))
                    }
                    "input_json_delta" => {
                        let partial = data
                            .pointer("/delta/partial_json")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let tool = self.tool_indices.get(&index).cloned().unwrap_or_default();
                        Ok(sse_data_line(&json!({
                            "id": self.message_id,
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": self.model,
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_calls": [{
                                        "index": index,
                                        "id": tool.id,
                                        "type": "function",
                                        "function": {
                                            "name": tool.name,
                                            "arguments": partial
                                        }
                                    }]
                                },
                                "finish_reason": Value::Null
                            }]
                        })))
                    }
                    _ => Ok(String::new()),
                }
            }
            "message_delta" => {
                self.usage = data.get("usage").cloned();
                let stop_reason =
                    map_anthropic_stop_reason(data.pointer("/delta/stop_reason").and_then(|v| v.as_str()));
                Ok(sse_data_line(&json!({
                    "id": self.message_id,
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": self.model,
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": stop_reason
                    }],
                    "usage": anthropic_usage_to_openai_usage(self.usage.as_ref())
                })))
            }
            "message_stop" => Ok("data: [DONE]\n\n".to_string()),
            "error" => Ok(sse_data_line(&json!({"error": data}))),
            _ => Ok(String::new()),
        }
    }
}

#[derive(Debug, Clone)]
struct OpenAiToolBlockState {
    anthropic_index: u32,
    id: String,
    name: String,
    started: bool,
    pending_args: String,
}

#[derive(Default)]
struct OpenAiChatToAnthropicState {
    message_started: bool,
    message_id: String,
    model: String,
    next_content_index: u32,
    current_non_tool_block_type: Option<&'static str>,
    current_non_tool_block_index: Option<u32>,
    tool_blocks_by_index: HashMap<usize, OpenAiToolBlockState>,
    open_tool_block_indices: HashSet<u32>,
}

impl OpenAiChatToAnthropicState {
    fn handle(&mut self, block: SseBlock) -> Result<String, String> {
        if block.data.len() == 1 && block.data[0].trim() == "[DONE]" {
            return Ok(sse_named_event("message_stop", &json!({"type":"message_stop"})));
        }
        let data = to_json_lines(&block.data)?;
        let mut out = String::new();

        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl_proxy");
        let model = data.get("model").and_then(|v| v.as_str()).unwrap_or("");
        if self.message_id.is_empty() {
            self.message_id = id.to_string();
        }
        if self.model.is_empty() {
            self.model = model.to_string();
        }
        if !self.message_started {
            out.push_str(&sse_named_event("message_start", &json!({
                "type":"message_start",
                "message": {
                    "id": self.message_id,
                    "type":"message",
                    "role":"assistant",
                    "model": self.model,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            })));
            self.message_started = true;
        }

        if let Some(choice) = data.get("choices").and_then(|v| v.as_array()).and_then(|arr| arr.first()) {
            if let Some(reasoning) = choice.pointer("/delta/reasoning").and_then(|v| v.as_str()) {
                out.push_str(&self.push_non_tool_delta("thinking", reasoning, true));
            }
            if let Some(content) = choice.pointer("/delta/content").and_then(|v| v.as_str()) {
                out.push_str(&self.push_non_tool_delta("text", content, false));
            }
            if let Some(tool_calls) = choice.pointer("/delta/tool_calls").and_then(|v| v.as_array()) {
                if let Some(index) = self.current_non_tool_block_index.take() {
                    out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
                }
                self.current_non_tool_block_type = None;
                for tool_call in tool_calls {
                    let tool_idx = tool_call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let state = self.tool_blocks_by_index.entry(tool_idx).or_insert_with(|| {
                        let anthropic_index = self.next_content_index;
                        self.next_content_index += 1;
                        OpenAiToolBlockState {
                            anthropic_index,
                            id: String::new(),
                            name: String::new(),
                            started: false,
                            pending_args: String::new(),
                        }
                    });
                    if let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) {
                        state.id = id.to_string();
                    }
                    if let Some(name) = tool_call.pointer("/function/name").and_then(|v| v.as_str()) {
                        state.name = name.to_string();
                    }
                    if !state.started && !state.id.is_empty() && !state.name.is_empty() {
                        state.started = true;
                        out.push_str(&sse_named_event("content_block_start", &json!({
                            "type":"content_block_start",
                            "index": state.anthropic_index,
                            "content_block":{"type":"tool_use","id":state.id,"name":state.name}
                        })));
                        self.open_tool_block_indices.insert(state.anthropic_index);
                        if !state.pending_args.is_empty() {
                            out.push_str(&sse_named_event("content_block_delta", &json!({
                                "type":"content_block_delta",
                                "index": state.anthropic_index,
                                "delta":{"type":"input_json_delta","partial_json":state.pending_args}
                            })));
                            state.pending_args.clear();
                        }
                    }
                    if let Some(arguments) = tool_call.pointer("/function/arguments").and_then(|v| v.as_str()) {
                        if state.started {
                            out.push_str(&sse_named_event("content_block_delta", &json!({
                                "type":"content_block_delta",
                                "index": state.anthropic_index,
                                "delta":{"type":"input_json_delta","partial_json":arguments}
                            })));
                        } else {
                            state.pending_args.push_str(arguments);
                        }
                    }
                }
            }
            if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                if let Some(index) = self.current_non_tool_block_index.take() {
                    out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
                }
                let mut tool_indices: Vec<u32> = self.open_tool_block_indices.iter().copied().collect();
                tool_indices.sort_unstable();
                for index in tool_indices {
                    out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
                }
                self.open_tool_block_indices.clear();
                out.push_str(&sse_named_event("message_delta", &json!({
                    "type":"message_delta",
                    "delta":{"stop_reason": map_openai_finish_reason(Some(finish_reason)), "stop_sequence": Value::Null},
                    "usage": openai_usage_to_anthropic_usage(data.get("usage"))
                })));
            }
        }

        Ok(out)
    }

    fn push_non_tool_delta(&mut self, kind: &'static str, text: &str, thinking: bool) -> String {
        let mut out = String::new();
        if self.current_non_tool_block_type != Some(kind) {
            if let Some(index) = self.current_non_tool_block_index.take() {
                out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
            }
            let index = self.next_content_index;
            self.next_content_index += 1;
            let content_block = if thinking {
                json!({"type":"thinking","thinking":""})
            } else {
                json!({"type":"text","text":""})
            };
            out.push_str(&sse_named_event("content_block_start", &json!({
                "type":"content_block_start",
                "index": index,
                "content_block": content_block
            })));
            self.current_non_tool_block_type = Some(kind);
            self.current_non_tool_block_index = Some(index);
        }
        if let Some(index) = self.current_non_tool_block_index {
            out.push_str(&sse_named_event("content_block_delta", &json!({
                "type":"content_block_delta",
                "index": index,
                "delta": if thinking {
                    json!({"type":"thinking_delta","thinking":text})
                } else {
                    json!({"type":"text_delta","text":text})
                }
            })));
        }
        out
    }
}

#[derive(Default)]
struct AnthropicToOpenAiResponsesState {
    message_id: String,
    model: String,
    usage: Option<Value>,
    text_item_open: bool,
    tool_items: HashMap<u64, String>,
    open_text_indices: HashSet<u64>,
    open_tool_indices: HashSet<u64>,
}

impl AnthropicToOpenAiResponsesState {
    fn handle(&mut self, block: SseBlock) -> Result<String, String> {
        let data = to_json_lines(&block.data)?;
        let mut out = String::new();
        match data.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "message_start" => {
                self.message_id = data.pointer("/message/id").and_then(|v| v.as_str()).unwrap_or("resp_proxy").to_string();
                self.model = data.pointer("/message/model").and_then(|v| v.as_str()).unwrap_or("").to_string();
                out.push_str(&sse_named_event("response.created", &json!({
                    "type":"response.created",
                    "response":{"id":self.message_id,"model":self.model,"usage": anthropic_usage_to_responses_usage(data.pointer("/message/usage"))}
                })));
            }
            "content_block_start" => {
                let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let block_type = data.pointer("/content_block/type").and_then(|v| v.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if !self.text_item_open {
                            out.push_str(&sse_named_event("response.output_item.added", &json!({
                                "type":"response.output_item.added",
                                "item":{"id":"msg_0","type":"message","role":"assistant"}
                            })));
                            self.text_item_open = true;
                        }
                        self.open_text_indices.insert(index);
                        out.push_str(&sse_named_event("response.content_part.added", &json!({
                            "type":"response.content_part.added",
                            "output_index":0,
                            "content_index": index,
                            "part":{"type":"output_text","text":""}
                        })));
                    }
                    "tool_use" => {
                        let item_id = format!("fc_{index}");
                        self.tool_items.insert(index, item_id.clone());
                        self.open_tool_indices.insert(index);
                        out.push_str(&sse_named_event("response.output_item.added", &json!({
                            "type":"response.output_item.added",
                            "item":{
                                "id": item_id,
                                "type":"function_call",
                                "call_id": data.pointer("/content_block/id").and_then(|v| v.as_str()).unwrap_or(""),
                                "name": data.pointer("/content_block/name").and_then(|v| v.as_str()).unwrap_or("")
                            }
                        })));
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let delta_type = data.pointer("/delta/type").and_then(|v| v.as_str()).unwrap_or("");
                match delta_type {
                    "text_delta" => out.push_str(&sse_named_event("response.output_text.delta", &json!({
                        "type":"response.output_text.delta",
                        "delta": data.pointer("/delta/text").and_then(|v| v.as_str()).unwrap_or(""),
                        "output_index": 0,
                        "content_index": index
                    }))),
                    "thinking_delta" => out.push_str(&sse_named_event("response.reasoning.delta", &json!({
                        "type":"response.reasoning.delta",
                        "delta": data.pointer("/delta/thinking").and_then(|v| v.as_str()).unwrap_or("")
                    }))),
                    "input_json_delta" => {
                        let item_id = self.tool_items.get(&index).cloned().unwrap_or_else(|| format!("fc_{index}"));
                        out.push_str(&sse_named_event("response.function_call_arguments.delta", &json!({
                            "type":"response.function_call_arguments.delta",
                            "item_id": item_id,
                            "delta": data.pointer("/delta/partial_json").and_then(|v| v.as_str()).unwrap_or("")
                        })));
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                if self.open_text_indices.remove(&index) {
                    out.push_str(&sse_named_event("response.content_part.done", &json!({
                        "type":"response.content_part.done",
                        "output_index":0,
                        "content_index": index
                    })));
                    out.push_str(&sse_named_event("response.output_text.done", &json!({
                        "type":"response.output_text.done",
                        "output_index":0,
                        "content_index": index
                    })));
                }
                if self.open_tool_indices.remove(&index) {
                    let item_id = self.tool_items.get(&index).cloned().unwrap_or_else(|| format!("fc_{index}"));
                    out.push_str(&sse_named_event("response.function_call_arguments.done", &json!({
                        "type":"response.function_call_arguments.done",
                        "item_id": item_id
                    })));
                    out.push_str(&sse_named_event("response.output_item.done", &json!({
                        "type":"response.output_item.done",
                        "item_id": item_id
                    })));
                }
            }
            "message_delta" => {
                self.usage = data.get("usage").cloned();
            }
            "message_stop" => {
                out.push_str(&sse_named_event("response.completed", &json!({
                    "type":"response.completed",
                    "response":{
                        "id": self.message_id,
                        "model": self.model,
                        "status": anthropic_stop_reason_to_responses_status(None),
                        "usage": anthropic_usage_to_responses_usage(self.usage.as_ref())
                    }
                })));
            }
            _ => {}
        }
        Ok(out)
    }
}

#[derive(Default)]
struct OpenAiResponsesToAnthropicState {
    next_content_index: u32,
    index_by_key: HashMap<String, u32>,
    open_indices: HashSet<u32>,
    fallback_open_index: Option<u32>,
    current_text_index: Option<u32>,
    tool_index_by_item_id: HashMap<String, u32>,
    has_tool_use: bool,
}

impl OpenAiResponsesToAnthropicState {
    fn handle(&mut self, block: SseBlock) -> Result<String, String> {
        let event_name = block.event.as_deref().unwrap_or("");
        let data = to_json_lines(&block.data)?;
        let mut out = String::new();
        match event_name {
            "response.created" => {
                let response_obj = data.get("response").unwrap_or(&data);
                out.push_str(&sse_named_event("message_start", &json!({
                    "type":"message_start",
                    "message": {
                        "id": response_obj.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        "type":"message",
                        "role":"assistant",
                        "model": response_obj.get("model").and_then(|v| v.as_str()).unwrap_or(""),
                        "usage": responses_usage_to_anthropic_usage(response_obj.get("usage"))
                    }
                })));
            }
            "response.content_part.added" => {
                let index = resolve_content_index(&data, &mut self.next_content_index, &mut self.index_by_key, &mut self.fallback_open_index);
                self.current_text_index = Some(index);
                if !self.open_indices.contains(&index) {
                    self.open_indices.insert(index);
                    out.push_str(&sse_named_event("content_block_start", &json!({
                        "type":"content_block_start",
                        "index": index,
                        "content_block":{"type":"text","text":""}
                    })));
                }
            }
            "response.output_text.delta" => {
                let index = self.current_text_index.unwrap_or_else(|| resolve_content_index(&data, &mut self.next_content_index, &mut self.index_by_key, &mut self.fallback_open_index));
                self.current_text_index = Some(index);
                if !self.open_indices.contains(&index) {
                    self.open_indices.insert(index);
                    out.push_str(&sse_named_event("content_block_start", &json!({
                        "type":"content_block_start",
                        "index": index,
                        "content_block":{"type":"text","text":""}
                    })));
                }
                out.push_str(&sse_named_event("content_block_delta", &json!({
                    "type":"content_block_delta",
                    "index": index,
                    "delta":{"type":"text_delta","text": data.get("delta").and_then(|v| v.as_str()).unwrap_or("")}
                })));
            }
            "response.output_text.done" | "response.content_part.done" => {
                if let Some(index) = self.current_text_index.take() {
                    if self.open_indices.remove(&index) {
                        out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
                    }
                    if self.fallback_open_index == Some(index) {
                        self.fallback_open_index = None;
                    }
                }
            }
            "response.output_item.added" => {
                if let Some(item) = data.get("item") {
                    if item.get("type").and_then(|v| v.as_str()) == Some("function_call") {
                        self.has_tool_use = true;
                        let index = self.next_content_index;
                        self.next_content_index += 1;
                        let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        self.tool_index_by_item_id.insert(item_id.to_string(), index);
                        out.push_str(&sse_named_event("content_block_start", &json!({
                            "type":"content_block_start",
                            "index": index,
                            "content_block":{
                                "type":"tool_use",
                                "id": item.get("call_id").and_then(|v| v.as_str()).unwrap_or(""),
                                "name": item.get("name").and_then(|v| v.as_str()).unwrap_or("")
                            }
                        })));
                        self.open_indices.insert(index);
                    }
                }
            }
            "response.function_call_arguments.delta" => {
                let item_id = data.get("item_id").and_then(|v| v.as_str()).unwrap_or("");
                let index = self.tool_index_by_item_id.get(item_id).copied().unwrap_or(0);
                out.push_str(&sse_named_event("content_block_delta", &json!({
                    "type":"content_block_delta",
                    "index": index,
                    "delta":{"type":"input_json_delta","partial_json": data.get("delta").and_then(|v| v.as_str()).unwrap_or("")}
                })));
            }
            "response.function_call_arguments.done" | "response.output_item.done" => {
                if let Some(item_id) = data.get("item_id").and_then(|v| v.as_str()) {
                    if let Some(index) = self.tool_index_by_item_id.get(item_id).copied() {
                        if self.open_indices.remove(&index) {
                            out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
                        }
                    }
                }
            }
            "response.reasoning.delta" => {
                let index = self.next_content_index;
                self.next_content_index += 1;
                out.push_str(&sse_named_event("content_block_start", &json!({
                    "type":"content_block_start",
                    "index": index,
                    "content_block":{"type":"thinking","thinking":""}
                })));
                out.push_str(&sse_named_event("content_block_delta", &json!({
                    "type":"content_block_delta",
                    "index": index,
                    "delta":{"type":"thinking_delta","thinking": data.get("delta").and_then(|v| v.as_str()).unwrap_or("")}
                })));
                out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
            }
            "response.completed" => {
                let response_obj = data.get("response").unwrap_or(&data);
                for index in self.open_indices.drain().collect::<Vec<_>>() {
                    out.push_str(&sse_named_event("content_block_stop", &json!({"type":"content_block_stop","index": index})));
                }
                out.push_str(&sse_named_event("message_delta", &json!({
                    "type":"message_delta",
                    "delta":{
                        "stop_reason": map_responses_status_to_anthropic(
                            response_obj.get("status").and_then(|v| v.as_str()),
                            self.has_tool_use,
                            response_obj.pointer("/incomplete_details/reason").and_then(|v| v.as_str())
                        ),
                        "stop_sequence": Value::Null
                    },
                    "usage": responses_usage_to_anthropic_usage(response_obj.get("usage"))
                })));
                out.push_str(&sse_named_event("message_stop", &json!({"type":"message_stop"})));
            }
            _ => {}
        }
        Ok(out)
    }
}

fn map_anthropic_stop_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("tool_use") => "tool_calls",
        Some("max_tokens") => "length",
        _ => "stop",
    }
}

fn map_openai_finish_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("tool_calls") | Some("function_call") => "tool_use",
        Some("length") => "max_tokens",
        _ => "end_turn",
    }
}

fn anthropic_usage_to_openai_usage(usage: Option<&Value>) -> Value {
    let usage = usage.cloned().unwrap_or_else(|| json!({}));
    let mut result = json!({
        "prompt_tokens": usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        "completion_tokens": usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
    });
    if let Some(cached) = usage.get("cache_read_input_tokens") {
        result["prompt_tokens_details"] = json!({"cached_tokens": cached});
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
    result
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
    result
}

fn anthropic_stop_reason_to_responses_status(stop_reason: Option<&str>) -> &'static str {
    match stop_reason {
        Some("max_tokens") => "incomplete",
        _ => "completed",
    }
}

fn map_responses_status_to_anthropic(
    status: Option<&str>,
    has_tool_use: bool,
    incomplete_reason: Option<&str>,
) -> &'static str {
    match status {
        Some("incomplete") => {
            if matches!(incomplete_reason, Some("content_filter")) {
                "end_turn"
            } else {
                "max_tokens"
            }
        }
        Some("completed") if has_tool_use => "tool_use",
        _ => "end_turn",
    }
}

fn resolve_content_index(
    data: &Value,
    next_content_index: &mut u32,
    index_by_key: &mut HashMap<String, u32>,
    fallback_open_index: &mut Option<u32>,
) -> u32 {
    let key = if let (Some(output_index), Some(content_index)) = (
        data.get("output_index").and_then(|v| v.as_u64()),
        data.get("content_index").and_then(|v| v.as_u64()),
    ) {
        Some(format!("{output_index}:{content_index}"))
    } else if let (Some(item_id), Some(content_index)) = (
        data.get("item_id").and_then(|v| v.as_str()),
        data.get("content_index").and_then(|v| v.as_u64()),
    ) {
        Some(format!("{item_id}:{content_index}"))
    } else {
        None
    };

    if let Some(key) = key {
        if let Some(index) = index_by_key.get(&key).copied() {
            index
        } else {
            let index = *next_content_index;
            *next_content_index += 1;
            index_by_key.insert(key, index);
            index
        }
    } else if let Some(index) = *fallback_open_index {
        index
    } else {
        let index = *next_content_index;
        *next_content_index += 1;
        *fallback_open_index = Some(index);
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stream_flag_detected() {
        assert!(is_stream_requested(&json!({"stream": true})));
        assert!(!is_stream_requested(&json!({"stream": false})));
        assert!(!is_stream_requested(&json!({})));
    }

    #[test]
    fn anthropic_stream_maps_to_openai_chat_sse() {
        let input = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-test\"}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let output = anthropic_sse_to_openai_chat(input).unwrap();
        assert!(output.contains("\"object\":\"chat.completion.chunk\""));
        assert!(output.contains("\"content\":\"Hello\""));
        assert!(output.contains("\"finish_reason\":\"stop\""));
        assert!(output.contains("[DONE]"));
    }

    #[test]
    fn openai_chat_stream_maps_to_anthropic_sse() {
        let input = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n"
        );

        let output = openai_chat_sse_to_anthropic(input).unwrap();
        assert!(output.contains("\"type\":\"message_start\""));
        assert!(output.contains("\"type\":\"text_delta\""));
        assert!(output.contains("\"text\":\"Hel\""));
        assert!(output.contains("\"text\":\"lo\""));
        assert!(output.contains("\"stop_reason\":\"end_turn\""));
    }

    #[test]
    fn anthropic_stream_maps_to_openai_responses_sse() {
        let input = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-test\"}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"lookup\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":\\\"hello\\\"}\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        let output = anthropic_sse_to_openai_responses(input).unwrap();
        assert!(output.contains("event: response.created"));
        assert!(output.contains("event: response.output_item.added"));
        assert!(output.contains("\"type\":\"function_call\""));
        assert!(output.contains("event: response.function_call_arguments.delta"));
        assert!(output.contains("event: response.completed"));
    }

    #[test]
    fn openai_responses_stream_maps_to_anthropic_sse() {
        let input = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-4o\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n",
            "event: response.content_part.added\n",
            "data: {\"type\":\"response.content_part.added\",\"part\":{\"type\":\"output_text\",\"text\":\"\"},\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\",\"output_index\":0,\"content_index\":0}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}}\n\n"
        );

        let output = openai_responses_sse_to_anthropic(input).unwrap();
        assert!(output.contains("\"type\":\"message_start\""));
        assert!(output.contains("\"type\":\"text_delta\""));
        assert!(output.contains("\"text\":\"Hello\""));
        assert!(output.contains("\"type\":\"message_stop\""));
    }
}
