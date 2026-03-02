use rusqlite::{Connection, Result};
use std::path::PathBuf;
use std::sync::Mutex;

static DB_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static DB_CONNECTION: std::sync::OnceLock<Mutex<Connection>> = std::sync::OnceLock::new();

fn get_db_path() -> PathBuf {
    let path = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("code-proxy")
        .join("providers.db");

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    path
}

pub fn get_database() -> Result<std::sync::MutexGuard<'static, Connection>, String> {
    let conn = DB_CONNECTION.get_or_init(|| {
        let path = get_db_path();
        let conn = Connection::open(&path)
            .expect("Failed to open database");
        Mutex::new(conn)
    });

    conn.lock().map_err(|e| format!("Failed to lock database: {}", e))
}

pub fn init_database() -> Result<(), String> {
    let conn = get_database()?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS providers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            remark TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            api_type TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).map_err(|e| format!("Failed to create table: {}", e))?;

    // Create settings table for app config
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    ).map_err(|e| format!("Failed to create settings table: {}", e))?;

    tracing::info!("Database initialized");
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare("SELECT value FROM settings WHERE key = ?")
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let value = stmt
        .query_row([key], |row| row.get(0))
        .ok();

    Ok(value)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
        [key, value],
    ).map_err(|e| format!("Failed to set setting: {}", e))?;

    Ok(())
}
