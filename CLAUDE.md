# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Code Proxy is a Claude Code proxy application that performs protocol conversion between OpenAI and Anthropic APIs. It allows users to switch between different AI providers while using Claude Code.

- **GUI Application**: Tauri 2 + React 19 + TypeScript (960x660 window)
- **Proxy Server**: Axum (Rust) on port 13721
- **MCP Server**: Integrated into GUI on port 13722
- **Database**: SQLite for provider storage
- **System Tray**: Built-in tray-icon support

## Build Commands

```bash
# Development
npm run tauri dev          # Run full dev environment

# Build
npm run tauri build        # Build release binaries
npm run build              # Build frontend only (tsc + vite)
cargo build --release      # Build Rust only

# Output
src-tauri/target/release/code-proxy.exe      # GUI app (includes MCP server)
```

## Architecture

### Frontend (`src/`)
- React 19 + TypeScript + Vite
- Tailwind CSS v4
- `src/lib/api.ts` - Tauri IPC bindings to Rust backend
- `src/lib/i18n/` - Internationalization support
- `src/locales/` - Language JSON files (en-US.json, zh-CN.json)

### Backend (`src-tauri/src/`)
- **lib.rs** - Tauri commands and app setup; defines `AppState` struct
- **proxy/** - HTTP proxy server
  - `server.rs` - Axum server with dynamic provider switching
  - `handlers.rs` - Request routing and model name replacement
  - `transform.rs` - OpenAI ↔ Anthropic protocol conversion
- **config.rs** - Claude Code `settings.json` management
- **database.rs** - SQLite provider and settings storage
- **provider.rs** - Provider data model and CRUD
- **mcp/** - MCP server (shares state with main app)
- **session_manager/** - Claude Code session scanning from `~/.claude/projects/`

## Key Patterns

### State Management
The app uses `AppState` struct with `Arc<RwLock<T>>` for shared state:
```rust
pub struct AppState {
    pub proxy_server: Arc<RwLock<Option<ProxyServer>>>,
    pub current_provider_id: Arc<RwLock<Option<i64>>>,
}
```
This state is shared between Tauri commands, proxy server, and MCP server.

### Tauri Commands
Add new commands in `lib.rs` with `#[tauri::command]` attribute:
```rust
#[tauri::command]
async fn my_command(state: tauri::State<'_, AppState>) -> Result<T, String> { ... }
```
Register in `invoke_handler(tauri::generate_handler![...])`.

### Adding New Tauri Commands
1. Define the command function in `lib.rs` with `#[tauri::command]`
2. Add to `generate_handler![]` macro
3. If needed, create frontend binding in `src/lib/api.ts`

## Database

- Location: `%LOCALAPPDATA%/code-proxy/providers.db`
- Tables:
  - `providers (id, name, remark, model, api_type, base_url, api_key, created_at, updated_at)`
  - `settings (key, value)` - App settings like current_provider_id, language

## Proxy Behavior

| API Type | Claude Code Request | Proxy Behavior |
|----------|-------------------|----------------|
| Anthropic Messages | Anthropic Messages | Pass-through |
| OpenAI Chat Completions | Anthropic Messages | Transform to OpenAI format |

### Model Name Replacement
Claude Code sends placeholder models (`code-default-model`, `code-haiku-model`, etc.) that are replaced with the provider's configured model.

## MCP Configuration

Add to `~/.claude.json`:
```json
{
  "mcpServers": {
    "code-proxy": {
      "type": "http",
      "url": "http://127.0.0.1:13722"
    }
  }
}
```

MCP tools: `list_providers`, `switch_provider`, `get_current_provider`, `proxy_status`

## Proxy Endpoints

- `POST /v1/chat/completions` - OpenAI protocol
- `POST /v1/messages` - Anthropic protocol
- `GET /health` - Health check
