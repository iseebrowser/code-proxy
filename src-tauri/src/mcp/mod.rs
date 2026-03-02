use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(json!({
                "code": code,
                "message": message
            })),
        }
    }
}

pub fn get_tools_definition() -> Value {
    json!({
        "tools": [
            {
                "name": "list_providers",
                "description": "List all available API providers",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "switch_provider",
                "description": "Switch to a different API provider",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "provider_id": {
                            "type": "number",
                            "description": "The ID of the provider to switch to"
                        }
                    },
                    "required": ["provider_id"]
                }
            },
            {
                "name": "get_current_provider",
                "description": "Get the currently active provider",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "proxy_status",
                "description": "Get the proxy server status",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

pub mod server;
