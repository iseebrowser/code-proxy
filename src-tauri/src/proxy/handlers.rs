use std::sync::Arc;
use axum::{extract::State, response::Response, body::Body, body::to_bytes};
use http::Request;
use tokio::sync::RwLock;
use crate::provider::Provider;
use super::transform::{transform_openai_to_anthropic, transform_anthropic_to_openai};

/// Model name mapping: Claude Code placeholder -> actual model
fn get_actual_model(provider_model: &str) -> String {
    // Map placeholder models to provider's configured model
    // All placeholder models map to the same provider model
    provider_model.to_string()
}

/// Replace model name in JSON body with provider's model
fn replace_model(body: &str, model: &str) -> String {
    // Match "model": "xxx" pattern and replace the value
    let re = regex::Regex::new(r#""model"\s*:\s*"[^"]*""#).unwrap();
    re.replace(body, format!(r#""model": "{}""#, model)).to_string()
}

pub async fn handle_chat_completion(
    State(provider): State<Arc<RwLock<Provider>>>,
    request: Request<Body>,
) -> Response<Body> {
    let provider = provider.read().await.clone();

    // Read request body
    let bytes = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&bytes);

    tracing::info!("Received OpenAI request: {}", body_str);

    // Replace model with provider's configured model
    let body_with_model = if provider.model.is_empty() {
        body_str.to_string()
    } else {
        replace_model(&body_str, &provider.model)
    };

    tracing::info!("Request with model replaced: {}", body_with_model);

    match provider.api_type.as_str() {
        "openai" => {
            // Pass through OpenAI to OpenAI
            forward_request(&provider, body_with_model.as_bytes(), "/v1/chat/completions").await
        }
        "anthropic" => {
            // Transform OpenAI to Anthropic
            match transform_openai_to_anthropic(&body_with_model) {
                Ok(transformed) => {
                    let response = forward_request(&provider, transformed.as_bytes(), "/v1/messages").await;
                    // Transform response back to OpenAI format
                    response
                }
                Err(e) => {
                    tracing::error!("Failed to transform request: {}", e);
                    Response::builder()
                        .status(400)
                        .body(Body::from(format!("{{\"error\": \"{}\"}}", e)))
                        .unwrap()
                }
            }
        }
        _ => {
            Response::builder()
                .status(400)
                .body(Body::from("{\"error\": \"Unknown provider type\"}"))
                .unwrap()
        }
    }
}

pub async fn handle_anthropic_message(
    State(provider): State<Arc<RwLock<Provider>>>,
    request: Request<Body>,
) -> Response<Body> {
    let provider = provider.read().await.clone();

    // Read request body
    let bytes = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8_lossy(&bytes);

    tracing::info!("Received Anthropic request: {}", body_str);

    // Replace model with provider's configured model
    let body_with_model = if provider.model.is_empty() {
        body_str.to_string()
    } else {
        replace_model(&body_str, &provider.model)
    };

    tracing::info!("Request with model replaced: {}", body_with_model);

    match provider.api_type.as_str() {
        "anthropic" => {
            // Pass through Anthropic to Anthropic
            forward_request(&provider, body_with_model.as_bytes(), "/v1/messages").await
        }
        "openai" => {
            // Transform Anthropic to OpenAI
            match transform_anthropic_to_openai(&body_with_model) {
                Ok(transformed) => {
                    forward_request(&provider, transformed.as_bytes(), "/v1/chat/completions").await
                }
                Err(e) => {
                    tracing::error!("Failed to transform request: {}", e);
                    Response::builder()
                        .status(400)
                        .body(Body::from(format!("{{\"error\": \"{}\"}}", e)))
                        .unwrap()
                }
            }
        }
        _ => {
            Response::builder()
                .status(400)
                .body(Body::from("{\"error\": \"Unknown provider type\"}"))
                .unwrap()
        }
    }
}

pub async fn health_check() -> Response<Body> {
    Response::builder()
        .status(200)
        .body(Body::from("{\"status\": \"ok\"}"))
        .unwrap()
}

async fn forward_request(
    provider: &Provider,
    body: &[u8],
    path: &str,
) -> Response<Body> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", provider.base_url.trim_end_matches('/'), path);

    tracing::info!("Forwarding request to: {}", url);

    match client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("x-api-key", provider.api_key.clone())
        .header("anthropic-version", "2023-06-01")
        .body(body.to_vec())
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            match response.text().await {
                Ok(text) => {
                    tracing::info!("Response status: {}, body: {}", status, text);
                    Response::builder()
                        .status(status.as_u16())
                        .body(Body::from(text))
                        .unwrap()
                }
                Err(e) => {
                    Response::builder()
                        .status(500)
                        .body(Body::from(format!("{{\"error\": \"{}\"}}", e)))
                        .unwrap()
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to forward request: {}", e);
            Response::builder()
                .status(500)
                .body(Body::from(format!("{{\"error\": \"{}\"}}", e)))
                .unwrap()
        }
    }
}
