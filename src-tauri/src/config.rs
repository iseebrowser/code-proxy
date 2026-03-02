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

    if enable {
        let env = json!({
            "ANTHROPIC_AUTH_TOKEN": "dummy",
            "ANTHROPIC_BASE_URL": format!("http://127.0.0.1:{}", PROXY_PORT),
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "code-haiku-model",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "code-opus-model",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "code-sonnet-model",
            "ANTHROPIC_SMALL_FAST_MODEL": "code-fast-model",
            "ANTHROPIC_MODEL": "code-default-model",
            "API_TIMEOUT_MS": "7200000",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "131072"
        });

        if let Some(obj) = config.as_object_mut() {
            obj.insert("env".to_string(), env);
        } else {
            config = json!({ "env": env });
        }
    } else {
        // Remove proxy configuration
        if let Some(obj) = config.as_object_mut() {
            obj.remove("env");
        }
    }

    write_claude_config(&config)?;
    tracing::info!("Claude config updated, enable: {}", enable);
    Ok(())
}

pub fn get_current_claude_config() -> Result<Value, String> {
    read_claude_config()
}
