use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    extract::State,
    response::Response,
};
use http::{HeaderMap, Request, StatusCode};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::provider::Provider;

use super::transform::{
    anthropic_to_openai_chat, anthropic_to_openai_chat_response, openai_chat_to_anthropic,
    openai_chat_to_anthropic_response,
};
use super::transform_responses::{
    anthropic_to_openai_responses, anthropic_to_openai_responses_response,
    openai_responses_to_anthropic, openai_responses_to_anthropic_response,
};
use super::streaming::{
    is_sse_content_type, is_stream_requested, streaming_response_from_reqwest, StreamRewrite,
};

const BODY_LIMIT: usize = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderApiType {
    Anthropic,
    OpenAiChat,
    OpenAiResponses,
}

#[derive(Clone, Copy, Debug)]
enum TargetEndpoint {
    AnthropicMessages,
    OpenAiChat,
    OpenAiResponses,
}

pub async fn handle_chat_completion(
    State(provider): State<Arc<RwLock<Provider>>>,
    request: Request<Body>,
) -> Response<Body> {
    let provider = provider.read().await.clone();
    match parse_json_request(request).await {
        Ok((headers, mut body)) => {
            let is_stream = is_stream_requested(&body);
            replace_model(&mut body, &provider.model);
            match normalized_api_type(&provider) {
                ProviderApiType::Anthropic => {
                    if is_stream {
                        match openai_chat_to_anthropic(body) {
                            Ok(outbound) => match send_upstream(
                                &provider,
                                &headers,
                                outbound,
                                TargetEndpoint::AnthropicMessages,
                            )
                            .await
                            {
                                Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                    return streaming_response_from_reqwest(
                                        response,
                                        StreamRewrite::AnthropicToOpenAiChat,
                                    );
                                }
                                Ok(response) => return upstream_non_sse_error(response).await,
                                Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
                        }
                    }
                    match openai_chat_to_anthropic(body) {
                        Ok(outbound) => match async_json_forward(
                            &provider,
                            &headers,
                            outbound,
                            TargetEndpoint::AnthropicMessages,
                        )
                        .await
                        {
                            Ok(response) => match anthropic_to_openai_chat_response(response) {
                                Ok(response) => json_response(StatusCode::OK, response),
                                Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                        },
                        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
                    }
                }
                ProviderApiType::OpenAiChat => {
                    if is_stream {
                        match send_upstream(&provider, &headers, body, TargetEndpoint::OpenAiChat).await {
                            Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                return streaming_response_from_reqwest(response, StreamRewrite::Passthrough);
                            }
                            Ok(response) => return upstream_non_sse_error(response).await,
                            Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                        }
                    }
                    match async_json_forward(&provider, &headers, body, TargetEndpoint::OpenAiChat).await {
                        Ok(response) => json_response(StatusCode::OK, response),
                        Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                    }
                }
                ProviderApiType::OpenAiResponses => error_response(
                    StatusCode::BAD_REQUEST,
                    "Provider uses OpenAI Responses API. Use /v1/responses instead.",
                ),
            }
        }
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub async fn handle_responses(
    State(provider): State<Arc<RwLock<Provider>>>,
    request: Request<Body>,
) -> Response<Body> {
    let provider = provider.read().await.clone();
    match parse_json_request(request).await {
        Ok((headers, mut body)) => {
            let is_stream = is_stream_requested(&body);
            replace_model(&mut body, &provider.model);
            match normalized_api_type(&provider) {
                ProviderApiType::Anthropic => {
                    if is_stream {
                        match openai_responses_to_anthropic(body) {
                            Ok(outbound) => match send_upstream(
                                &provider,
                                &headers,
                                outbound,
                                TargetEndpoint::AnthropicMessages,
                            )
                            .await
                            {
                                Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                    return streaming_response_from_reqwest(
                                        response,
                                        StreamRewrite::AnthropicToOpenAiResponses,
                                    );
                                }
                                Ok(response) => return upstream_non_sse_error(response).await,
                                Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
                        }
                    }
                    match openai_responses_to_anthropic(body) {
                        Ok(outbound) => match async_json_forward(
                            &provider,
                            &headers,
                            outbound,
                            TargetEndpoint::AnthropicMessages,
                        )
                        .await
                        {
                            Ok(response) => match anthropic_to_openai_responses_response(response) {
                                Ok(response) => json_response(StatusCode::OK, response),
                                Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                        },
                        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
                    }
                }
                ProviderApiType::OpenAiResponses => {
                    if is_stream {
                        match send_upstream(&provider, &headers, body, TargetEndpoint::OpenAiResponses).await {
                            Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                return streaming_response_from_reqwest(response, StreamRewrite::Passthrough);
                            }
                            Ok(response) => return upstream_non_sse_error(response).await,
                            Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                        }
                    }
                    match async_json_forward(&provider, &headers, body, TargetEndpoint::OpenAiResponses).await {
                        Ok(response) => json_response(StatusCode::OK, response),
                        Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                    }
                }
                ProviderApiType::OpenAiChat => error_response(
                    StatusCode::BAD_REQUEST,
                    "Provider uses OpenAI Chat Completions. Use /v1/chat/completions instead.",
                ),
            }
        }
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub async fn handle_anthropic_message(
    State(provider): State<Arc<RwLock<Provider>>>,
    request: Request<Body>,
) -> Response<Body> {
    let provider = provider.read().await.clone();
    match parse_json_request(request).await {
        Ok((headers, mut body)) => {
            let is_stream = is_stream_requested(&body);
            replace_model(&mut body, &provider.model);
            match normalized_api_type(&provider) {
                ProviderApiType::Anthropic => {
                    if is_stream {
                        match send_upstream(&provider, &headers, body, TargetEndpoint::AnthropicMessages).await {
                            Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                return streaming_response_from_reqwest(response, StreamRewrite::Passthrough);
                            }
                            Ok(response) => return upstream_non_sse_error(response).await,
                            Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                        }
                    }
                    match async_json_forward(&provider, &headers, body, TargetEndpoint::AnthropicMessages).await {
                        Ok(response) => json_response(StatusCode::OK, response),
                        Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                    }
                }
                ProviderApiType::OpenAiChat => {
                    if is_stream {
                        match anthropic_to_openai_chat(body) {
                            Ok(outbound) => match send_upstream(
                                &provider,
                                &headers,
                                outbound,
                                TargetEndpoint::OpenAiChat,
                            )
                            .await
                            {
                                Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                    return streaming_response_from_reqwest(
                                        response,
                                        StreamRewrite::OpenAiChatToAnthropic,
                                    );
                                }
                                Ok(response) => return upstream_non_sse_error(response).await,
                                Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
                        }
                    }
                    match anthropic_to_openai_chat(body) {
                        Ok(outbound) => match async_json_forward(
                            &provider,
                            &headers,
                            outbound,
                            TargetEndpoint::OpenAiChat,
                        )
                        .await
                        {
                            Ok(response) => match openai_chat_to_anthropic_response(response) {
                                Ok(response) => json_response(StatusCode::OK, response),
                                Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                        },
                        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
                    }
                }
                ProviderApiType::OpenAiResponses => {
                    if is_stream {
                        match anthropic_to_openai_responses(body) {
                            Ok(outbound) => match send_upstream(
                                &provider,
                                &headers,
                                outbound,
                                TargetEndpoint::OpenAiResponses,
                            )
                            .await
                            {
                                Ok(response) if is_sse_content_type(response.headers().get("content-type")) => {
                                    return streaming_response_from_reqwest(
                                        response,
                                        StreamRewrite::OpenAiResponsesToAnthropic,
                                    );
                                }
                                Ok(response) => return upstream_non_sse_error(response).await,
                                Err(error) => return error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => return error_response(StatusCode::BAD_REQUEST, &error),
                        }
                    }
                    match anthropic_to_openai_responses(body) {
                        Ok(outbound) => match async_json_forward(
                            &provider,
                            &headers,
                            outbound,
                            TargetEndpoint::OpenAiResponses,
                        )
                        .await
                        {
                            Ok(response) => match openai_responses_to_anthropic_response(response) {
                                Ok(response) => json_response(StatusCode::OK, response),
                                Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                            },
                            Err(error) => error_response(StatusCode::BAD_GATEWAY, &error),
                        },
                        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
                    }
                }
            }
        }
        Err(error) => error_response(StatusCode::BAD_REQUEST, &error),
    }
}

pub async fn health_check() -> Response<Body> {
    json_response(StatusCode::OK, json!({ "status": "ok" }))
}

async fn parse_json_request(request: Request<Body>) -> Result<(HeaderMap, Value), String> {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, BODY_LIMIT)
        .await
        .map_err(|e| format!("Failed to read request body: {e}"))?;
    let value = serde_json::from_slice::<Value>(&bytes)
        .map_err(|e| format!("Failed to parse JSON body: {e}"))?;
    Ok((parts.headers, value))
}

async fn async_json_forward(
    provider: &Provider,
    incoming_headers: &HeaderMap,
    body: Value,
    endpoint: TargetEndpoint,
) -> Result<Value, String> {
    let response = send_upstream(provider, incoming_headers, body, endpoint).await?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if !status.is_success() {
        return Err(format!("Upstream returned {status}: {text}"));
    }

    serde_json::from_str::<Value>(&text)
        .map_err(|e| format!("Failed to parse upstream JSON response: {e}; body={text}"))
}

async fn send_upstream(
    provider: &Provider,
    incoming_headers: &HeaderMap,
    body: Value,
    endpoint: TargetEndpoint,
) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}{}",
        provider.base_url.trim_end_matches('/'),
        endpoint_path(endpoint)
    );

    let body_bytes = serde_json::to_vec(&body).map_err(|e| format!("Failed to encode JSON: {e}"))?;
    let mut builder = client.post(url).body(body_bytes);

    for (name, value) in incoming_headers {
        if should_forward_header(name.as_str()) {
            builder = builder.header(name, value);
        }
    }

    builder = apply_emulated_headers(builder, provider, endpoint);

    builder
        .send()
        .await
        .map_err(|e| format!("Request forwarding failed: {e}"))
}

async fn upstream_non_sse_error(response: reqwest::Response) -> Response<Body> {
    let status = response.status();
    let text = response
        .text()
        .await
        .unwrap_or_else(|e| format!("Failed to read upstream response: {e}"));
    error_response(status, &format!("Expected SSE upstream response but received: {text}"))
}

fn apply_emulated_headers(
    mut builder: reqwest::RequestBuilder,
    provider: &Provider,
    endpoint: TargetEndpoint,
) -> reqwest::RequestBuilder {
    builder = builder
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("accept-encoding", "identity")
        .header("accept-language", "*")
        .header("user-agent", "claude-cli/2.1.72 (external, cli)")
        .header("x-app", "cli")
        .header("x-stainless-lang", "js")
        .header("x-stainless-package-version", "0.70.0")
        .header("x-stainless-os", get_stainless_os())
        .header("x-stainless-arch", get_stainless_arch())
        .header("x-stainless-runtime", "node")
        .header("x-stainless-runtime-version", "v22.20.0")
        .header("x-stainless-retry-count", "0")
        .header("x-stainless-timeout", "600");

    match endpoint {
        TargetEndpoint::AnthropicMessages => {
            builder
                .header("x-api-key", provider.api_key.clone())
                .header("anthropic-version", "2023-06-01")
                .header(
                    "anthropic-beta",
                    "claude-code-20250219,interleaved-thinking-2025-05-14",
                )
                .header("anthropic-dangerous-direct-browser-access", "true")
        }
        TargetEndpoint::OpenAiChat | TargetEndpoint::OpenAiResponses => builder.header(
            "authorization",
            format!("Bearer {}", provider.api_key),
        ),
    }
}

fn endpoint_path(endpoint: TargetEndpoint) -> &'static str {
    match endpoint {
        TargetEndpoint::AnthropicMessages => "/v1/messages",
        TargetEndpoint::OpenAiChat => "/v1/chat/completions",
        TargetEndpoint::OpenAiResponses => "/v1/responses",
    }
}

fn normalized_api_type(provider: &Provider) -> ProviderApiType {
    match provider.api_type.as_str() {
        "anthropic" => ProviderApiType::Anthropic,
        "openai_responses" => ProviderApiType::OpenAiResponses,
        "openai_chat" | "openai" => ProviderApiType::OpenAiChat,
        _ => ProviderApiType::Anthropic,
    }
}

fn replace_model(body: &mut Value, model: &str) {
    if model.is_empty() {
        return;
    }
    if let Some(obj) = body.as_object_mut() {
        if obj.contains_key("model") {
            obj.insert("model".to_string(), json!(model));
        }
    }
}

fn should_forward_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "x-request-id" | "x-trace-id" | "anthropic-beta"
    )
}

fn json_response(status: StatusCode, body: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn error_response(status: StatusCode, message: &str) -> Response<Body> {
    json_response(status, json!({ "error": message }))
}

fn get_stainless_os() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Windows"
    }
    #[cfg(target_os = "macos")]
    {
        "MacOS"
    }
    #[cfg(target_os = "linux")]
    {
        "Linux"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "Unknown"
    }
}

fn get_stainless_arch() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(target_arch = "x86")]
    {
        "x86"
    }
    #[cfg(target_arch = "arm")]
    {
        "arm"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86", target_arch = "arm")))]
    {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_openai_type_maps_to_chat() {
        let provider = Provider {
            id: 1,
            name: "test".to_string(),
            remark: String::new(),
            model: String::new(),
            api_type: "openai".to_string(),
            base_url: "https://example.com".to_string(),
            api_key: "k".to_string(),
        };

        assert_eq!(normalized_api_type(&provider), ProviderApiType::OpenAiChat);
    }

    #[test]
    fn replace_model_updates_existing_model_key() {
        let mut body = json!({"model": "code-default-model", "messages": []});
        replace_model(&mut body, "gpt-4o");
        assert_eq!(body["model"], "gpt-4o");
    }
}
