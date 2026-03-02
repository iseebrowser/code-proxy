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
- `src/components/SessionManager.tsx` - Session management dialog
- `src/lib/i18n/` - Internationalization (i18n) support
- `src/locales/` - Language JSON files (en-US.json, zh-CN.json)

### Backend (`src-tauri/src/`)
- **proxy/** - HTTP proxy server
  - `server.rs` - Axum server startup/shutdown with dynamic provider switching
  - `handlers.rs` - Request routing, forwarding, and model name replacement
  - `transform.rs` - OpenAI ↔ Anthropic protocol conversion
- **config.rs** - Claude Code `settings.json` management
- **database.rs** - SQLite provider and settings storage
- **provider.rs** - Provider data model and CRUD
- **mcp/** - MCP server (integrated into GUI app)
- **session_manager/** - Claude Code session management
  - Scans and manages Claude Code sessions from `~/.claude/projects/`
  - Supports session browsing, viewing message history, and deleting sessions

## Database

- Location: `%LOCALAPPDATA%/code-proxy/providers.db`
- Tables:
  - `providers (id, name, remark, model, api_type, base_url, api_key, created_at, updated_at)`
  - `settings (key, value)` - App settings like current_provider_id, language

## Features

### System Tray
- Application runs in system tray (Windows)
- Left/Right click shows menu with:
  - "打开主界面" / "Show Main Window" - Show main window (localized)
  - Provider list (current provider checked)
  - "退出" / "Quit" - Exit application (localized)
- Double-click tray icon to show main window
- Main window close button hides to tray instead of exiting

### Provider Sync
- Main window and System Tray provider selection are synced
- Current provider shown with checkmark in both UI and tray menu
- Switching provider in either place updates the other

### Auto-save Provider Selection
- Selected provider is persisted to database
- On app restart, automatically loads the last selected provider

### Auto-start Proxy
- If a provider was selected before closing the app, proxy starts automatically on launch
- Automatically updates Claude Code `settings.json` on auto-start
- Button correctly shows "Stop Proxy" state

### Dynamic Provider Switching
- While proxy is running, selecting a different provider switches immediately
- No need to restart proxy for provider changes

### Model Name Replacement
- Claude Code sends placeholder model names:
  - `code-default-model` → provider's configured model
  - `code-haiku-model` → provider's configured model
  - `code-opus-model` → provider's configured model
  - `code-sonnet-model` → provider's configured model
  - `code-fast-model` → provider's configured model
- Proxy automatically replaces all placeholder models with the actual model configured in provider

### Session Management
- Browse and manage Claude Code sessions from `~/.claude/projects/`
- View session message history
- Delete sessions (removes session file from disk)
- "Manage Sessions" button in main UI opens session manager dialog

### Provider Management
- Delete provider requires confirmation dialog
- Edit and Delete buttons are disabled (grayed out) for the currently selected provider

### Internationalization (i18n)
- Supports Chinese (zh-CN) and English (en-US) UI languages
- Language JSON files in `src/locales/` directory
- Auto-detects system language on first launch and saves as default
- Language preference persisted to database (settings table)
- Language selector in toolbar (EN/中文 button)
- System tray menu also localized based on language setting

## Provider Form Fields

- **Provider**: Model name (e.g., ANTHROPIC_MODEL, ANTHROPIC_DEFAULT_HAIKU_MODEL)
- **Remark**: Optional description
- **API Key**: ANTHROPIC_AUTH_TOKEN
- **Base URL**: ANTHROPIC_BASE_URL (e.g., https://api.anthropic.com)
- **API Type**: Anthropic Messages / OpenAI Chat Completions

## Proxy Behavior

| API Type | Claude Code Request | Proxy Behavior |
|----------|-------------------|----------------|
| Anthropic Messages | Anthropic Messages | Pass-through (no conversion) |
| OpenAI Chat Completions | Anthropic Messages | Transform to OpenAI format |

- **Anthropic Messages**: Pass-through mode - directly forwards requests to the target API
- **OpenAI Chat Completions**: Converts Claude Code's Anthropic Messages to OpenAI Chat Completions format, then forwards to target API
- **Model Replacement**: Automatically replaces placeholder model names with provider's configured model

## MCP Server Configuration

Use the `claude mcp add` command to register the MCP server:

```bash
claude mcp add --transport http code-proxy http://127.0.0.1:13722
```

Or manually add to `~/.claude.json`:

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

### MCP Tools

- `list_providers` - List all configured providers
- `switch_provider` - Switch to a different provider (by provider_id)
- `get_current_provider` - Get active provider info
- `proxy_status` - Check if proxy is running

### MCP Provider Switching

When switching providers via MCP:
1. Main window dropdown updates automatically
2. System Tray menu updates automatically (shows checkmark on selected provider)
3. Proxy continues running with new provider configuration

## Proxy Endpoints

- `POST /v1/chat/completions` - OpenAI protocol
- `POST /v1/messages` - Anthropic protocol
- `GET /health` - Health check
