use crate::database;
use crate::provider;
use crate::proxy;
use crate::config;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tauri::{AppHandle, Emitter};

use crate::mcp::{get_tools_definition, JsonRpcRequest, JsonRpcResponse};

const MCP_PORT: u16 = 13722;

pub struct McpState {
    pub proxy_server: Arc<RwLock<Option<proxy::server::ProxyServer>>>,
    pub current_provider_id: Arc<RwLock<Option<i64>>>,
    pub app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl McpState {
    pub fn new() -> Self {
        Self {
            proxy_server: Arc::new(RwLock::new(None)),
            current_provider_id: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }
}

pub async fn run_mcp_server(
    state: Arc<McpState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("127.0.0.1:{}", MCP_PORT);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("MCP server listening on {}", addr);

    loop {
        let (socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, state).await {
                tracing::error!("MCP connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(
    mut socket: TcpStream,
    state: Arc<McpState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (reader, mut writer) = socket.split();
    let mut reader = tokio::io::BufReader::new(reader);

    // Read HTTP request line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(());
    }

    // Check if it's an HTTP request
    if request_line.starts_with("GET") || request_line.starts_with("POST") {
        // Read headers
        let mut content_length = 0;
        loop {
            let mut header_line = String::new();
            reader.read_line(&mut header_line).await?;
            if header_line.trim().is_empty() {
                break; // End of headers
            }
            if header_line.to_lowercase().starts_with("content-length:") {
                content_length = header_line.split(':')
                    .nth(1)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
            }
        }

        // Read body for POST requests
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).await?;
        }

        let body_str = String::from_utf8_lossy(&body);

        // Handle HTTP request
        if request_line.starts_with("POST") && !body_str.is_empty() {
            // Parse JSON-RPC request
            let request: JsonRpcRequest = match serde_json::from_str(&body_str) {
                Ok(r) => r,
                Err(e) => {
                    let error_response = JsonRpcResponse::error(json!(0), -32700, &e.to_string());
                    send_http_response(&mut writer, &error_response).await?;
                    return Ok(());
                }
            };

            let response = handle_request(request, state).await;
            send_http_response(&mut writer, &response).await?;
        } else if request_line.starts_with("GET") && request_line.contains("/health") {
            // Health check endpoint
            let response = json!({"status": "ok"});
            send_http_json_response(&mut writer, &response).await?;
        } else {
            // Return tools list for GET or empty POST
            let tools = get_tools_definition();
            send_http_json_response(&mut writer, &tools).await?;
        }
    } else {
        // Legacy TCP mode (for backwards compatibility)
        let mut line = request_line;

        // Send initialize response
        let init_response = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "code-proxy",
                "version": "0.1.0"
            }
        });

        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            result: Some(init_response),
            error: None,
        };
        writer.write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes()).await?;
        writer.flush().await?;

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // Connection closed
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("Read error: {}", e);
                    break;
                }
            };

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let error_response = JsonRpcResponse::error(json!(0), -32700, &e.to_string());
                    writer.write_all(format!("{}\n", serde_json::to_string(&error_response)?).as_bytes()).await?;
                    writer.flush().await?;
                    continue;
                }
            };

            let response = handle_request(request, state.clone()).await;
            writer.write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes()).await?;
            writer.flush().await?;
        }
    }

    Ok(())
}

async fn send_http_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: &JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let body = serde_json::to_string(response)?;
    let length = body.len();
    let http_response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        length, body
    );
    writer.write_all(http_response.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_http_json_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let body = serde_json::to_string(response)?;
    let length = body.len();
    let http_response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        length, body
    );
    writer.write_all(http_response.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn handle_request(request: JsonRpcRequest, state: Arc<McpState>) -> JsonRpcResponse {
    match request.method.as_str() {
        "initialize" => {
            let result = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "code-proxy",
                    "version": "0.1.0"
                }
            });
            JsonRpcResponse::success(request.id, result)
        }
        "tools/list" => {
            let tools = get_tools_definition();
            JsonRpcResponse::success(request.id, tools)
        }
        "tools/call" => {
            let params = request.params;
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");

            let result = match tool_name {
                "list_providers" => {
                    match list_providers_handler() {
                        Ok(providers) => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string(&providers).unwrap_or_default()
                                }
                            ]
                        }),
                        Err(e) => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!("Error: {}", e)
                                }
                            ],
                            "isError": true
                        })
                    }
                }
                "switch_provider" => {
                    let provider_id = params
                        .get("arguments")
                        .and_then(|v| v.get("provider_id"))
                        .and_then(|v| v.as_i64());

                    match provider_id {
                        Some(id) => match switch_provider_handler(id, state.clone()).await {
                            Ok(_) => json!({
                                "content": [
                                    {
                                        "type": "text",
                                        "text": format!("Switched to provider {}", id)
                                    }
                                ]
                            }),
                            Err(e) => json!({
                                "content": [
                                    {
                                        "type": "text",
                                        "text": format!("Error: {}", e)
                                    }
                                ],
                                "isError": true
                            })
                        },
                        None => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": "Missing provider_id parameter"
                                }
                            ],
                            "isError": true
                        })
                    }
                }
                "get_current_provider" => {
                    match get_current_provider_handler(state).await {
                        Ok(provider) => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string(&provider).unwrap_or_else(|_| "null".to_string())
                                }
                            ]
                        }),
                        Err(e) => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!("Error: {}", e)
                                }
                            ],
                            "isError": true
                        })
                    }
                }
                "proxy_status" => {
                    match get_proxy_status_handler(state).await {
                        Ok(status) => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!("Proxy running: {}", status)
                                }
                            ]
                        }),
                        Err(e) => json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!("Error: {}", e)
                                }
                            ],
                            "isError": true
                        })
                    }
                }
                _ => {
                    json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!("Unknown tool: {}", tool_name)
                            }
                        ],
                        "isError": true
                    })
                }
            };
            JsonRpcResponse::success(request.id, result)
        }
        _ => JsonRpcResponse::error(request.id, -32601, "Method not found"),
    }
}

fn list_providers_handler() -> Result<Vec<provider::Provider>, String> {
    let db = database::get_database()?;
    provider::list_providers(&db).map_err(|e| e.to_string())
}

async fn switch_provider_handler(
    provider_id: i64,
    state: Arc<McpState>,
) -> Result<(), String> {
    // Get provider from database
    let provider = {
        let db = database::get_database()?;
        provider::get_provider(&db, provider_id)
            .map_err(|e| e.to_string())?
            .ok_or("Provider not found")?
    };

    // Stop existing proxy
    {
        let mut server_lock = state.proxy_server.write().await;
        if let Some(mut server) = server_lock.take() {
            let _ = server.stop().await;
        }
    }

    // Start new proxy
    let mut server = proxy::server::ProxyServer::new(provider);
    server.start().map_err(|e| e.to_string())?;

    // Update Claude config
    config::update_claude_config(true).map_err(|e| e.to_string())?;

    // Update state
    {
        let mut server_lock = state.proxy_server.write().await;
        *server_lock = Some(server);
    }
    {
        let mut provider_lock = state.current_provider_id.write().await;
        *provider_lock = Some(provider_id);
    }

    // Save to database
    {
        let db = database::get_database()?;
        database::set_setting(&db, "current_provider_id", &provider_id.to_string())
            .map_err(|e| e.to_string())?;
    }

    // Emit event to refresh tray menu and frontend
    {
        let app_handle_guard = state.app_handle.lock().unwrap();
        if let Some(app_handle) = app_handle_guard.as_ref() {
            let app_handle = app_handle.clone();
            let provider_id_clone = provider_id;
            std::thread::spawn(move || {
                // Rebuild tray menu
                if let Some(tray) = app_handle.tray_by_id("main") {
                    if let Ok(menu) = crate::build_tray_menu(&app_handle) {
                        let _ = tray.set_menu(Some(menu));
                    }
                }
                // Emit event to frontend
                let _ = app_handle.emit("provider-changed", provider_id_clone);
            });
        }
    }

    Ok(())
}

async fn get_current_provider_handler(
    state: Arc<McpState>,
) -> Result<Option<provider::Provider>, String> {
    let provider_id = state.current_provider_id.read().await;
    if let Some(id) = *provider_id {
        let db = database::get_database()?;
        provider::get_provider(&db, id).map_err(|e| e.to_string())
    } else {
        Ok(None)
    }
}

async fn get_proxy_status_handler(
    state: Arc<McpState>,
) -> Result<bool, String> {
    let server_lock = state.proxy_server.read().await;
    Ok(server_lock.is_some())
}
