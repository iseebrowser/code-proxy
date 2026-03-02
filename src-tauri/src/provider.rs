use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: i64,
    pub name: String,
    pub remark: String,
    pub model: String,
    pub api_type: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInput {
    pub name: String,
    pub remark: String,
    pub model: String,
    pub api_type: String,
    pub base_url: String,
    pub api_key: String,
}

pub fn list_providers(conn: &Connection) -> Result<Vec<Provider>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, remark, model, api_type, base_url, api_key FROM providers ORDER BY id")
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let providers = stmt
        .query_map([], |row| {
            Ok(Provider {
                id: row.get(0)?,
                name: row.get(1)?,
                remark: row.get(2)?,
                model: row.get(3)?,
                api_type: row.get(4)?,
                base_url: row.get(5)?,
                api_key: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query providers: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(providers)
}

pub fn get_provider(conn: &Connection, id: i64) -> Result<Option<Provider>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, remark, model, api_type, base_url, api_key FROM providers WHERE id = ?")
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let provider = stmt
        .query_row([id], |row| {
            Ok(Provider {
                id: row.get(0)?,
                name: row.get(1)?,
                remark: row.get(2)?,
                model: row.get(3)?,
                api_type: row.get(4)?,
                base_url: row.get(5)?,
                api_key: row.get(6)?,
            })
        })
        .ok();

    Ok(provider)
}

pub fn add_provider(conn: &Connection, input: ProviderInput) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO providers (name, remark, model, api_type, base_url, api_key) VALUES (?, ?, ?, ?, ?, ?)",
        [&input.name, &input.remark, &input.model, &input.api_type, &input.base_url, &input.api_key],
    ).map_err(|e| format!("Failed to insert provider: {}", e))?;

    let id = conn.last_insert_rowid();
    tracing::info!("Added provider: {} (id: {})", input.name, id);
    Ok(id)
}

pub fn update_provider(conn: &Connection, id: i64, input: ProviderInput) -> Result<(), String> {
    conn.execute(
        "UPDATE providers SET name = ?, remark = ?, model = ?, api_type = ?, base_url = ?, api_key = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        rusqlite::params![input.name, input.remark, input.model, input.api_type, input.base_url, input.api_key, id],
    ).map_err(|e| format!("Failed to update provider: {}", e))?;

    tracing::info!("Updated provider: {} (id: {})", input.name, id);
    Ok(())
}

pub fn delete_provider(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM providers WHERE id = ?", [id])
        .map_err(|e| format!("Failed to delete provider: {}", e))?;

    tracing::info!("Deleted provider id: {}", id);
    Ok(())
}
