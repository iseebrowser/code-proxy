use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const PROXY_PORT: u16 = 13721;

pub fn get_claude_config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot find home directory")
        .join(".claude")
}

fn get_claude_config_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or("Cannot find home directory")?;
    Ok(home.join(".Claude").join("settings.json"))
}

fn read_claude_config() -> Result<Value, String> {
    let path = get_claude_config_path()?;
    if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))
    } else {
        Ok(json!({}))
    }
}

fn write_claude_config(config: &Value) -> Result<(), String> {
    let path = get_claude_config_path()?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

pub fn update_claude_config(enable: bool) -> Result<(), String> {
    let mut config = read_claude_config()?;

    // Proxy-related environment variables
    let proxy_env_keys = [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_SMALL_FAST_MODEL",
        "ANTHROPIC_MODEL",
        "API_TIMEOUT_MS",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    ];

    if enable {
        // Ensure config is an object
        if !config.is_object() {
            config = json!({});
        }

        // Get or create env object
        let env = if let Some(obj) = config.as_object_mut() {
            obj.entry("env").or_insert_with(|| json!({}))
        } else {
            return Err("Config is not an object".to_string());
        };

        // Add/update only proxy-related env vars, preserving other env vars
        if let Some(env_obj) = env.as_object_mut() {
            env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!("dummy"));
            env_obj.insert("ANTHROPIC_BASE_URL".to_string(), json!(format!("http://127.0.0.1:{}", PROXY_PORT)));
            env_obj.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), json!("code-haiku-model"));
            env_obj.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), json!("code-opus-model"));
            env_obj.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), json!("code-sonnet-model"));
            env_obj.insert("ANTHROPIC_SMALL_FAST_MODEL".to_string(), json!("code-fast-model"));
            env_obj.insert("ANTHROPIC_MODEL".to_string(), json!("code-default-model"));
            env_obj.insert("API_TIMEOUT_MS".to_string(), json!("7200000"));
            env_obj.insert("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(), json!("1"));
            env_obj.insert("CLAUDE_CODE_MAX_OUTPUT_TOKENS".to_string(), json!("131072"));
        }
    } else {
        // Remove only proxy-related env vars, preserving other env vars
        if let Some(obj) = config.as_object_mut() {
            if let Some(env) = obj.get_mut("env") {
                if let Some(env_obj) = env.as_object_mut() {
                    for key in &proxy_env_keys {
                        env_obj.remove(*key);
                    }
                }
            }
        }
    }

    write_claude_config(&config)?;
    tracing::info!("Claude config updated, enable: {}", enable);
    Ok(())
}

pub fn get_current_claude_config() -> Result<Value, String> {
    read_claude_config()
}
