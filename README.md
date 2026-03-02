# Code Proxy

A Claude Code proxy application that performs protocol conversion between OpenAI and Anthropic APIs. Allows users to switch between different AI providers while using Claude Code.

## Features

- **Protocol Conversion**: Convert between OpenAI Chat Completions and Anthropic Messages formats
- **Multi-Provider Support**: Manage multiple API providers with SQLite storage
- **Dynamic Switching**: Switch providers without restarting the proxy
- **Auto-Update Config**: Automatically updates Claude Code `settings.json`
- **MCP Server**: Built-in MCP server for provider management
- **System Tray**: Runs in system tray with quick access menu
- **Internationalization**: Supports Chinese and English UI languages
- **Session Management**: Browse and manage Claude Code sessions

## Architecture

- **GUI Application**: Tauri 2 + React 19 + TypeScript (960x660 window)
- **Proxy Server**: Axum (Rust) on port 13721
- **MCP Server**: Integrated into GUI on port 13722
- **Database**: SQLite for provider storage

## Installation

### Build from Source

```bash
# Install dependencies
npm install

# Development
npm run tauri dev

# Build release
npm run tauri build
```

The executable will be at:
- `src-tauri/target/release/code-proxy.exe` (Windows)

### Quick Start

1. Run `code-proxy.exe`
2. Add your API provider (API Key, Base URL, Model name)
3. Click "Start Proxy" to enable the proxy
4. Configure Claude Code to use the proxy

## Configuration

### Claude Code Settings

Edit `~/.claude.json` to add the proxy and MCP server:

```json
{
  "apiUrl": "http://127.0.0.1:13721",
  "mcpServers": {
    "code-proxy": {
      "url": "http://127.0.0.1:13722"
    }
  }
}
```

Or use the `claude mcp add` command:

```bash
claude mcp add --transport http code-proxy http://127.0.0.1:13722
```

## MCP Tools

- `list_providers` - List all configured providers
- `switch_provider` - Switch to a different provider (by provider_id)
- `get_current_provider` - Get active provider info
- `proxy_status` - Check if proxy is running

## Proxy Endpoints

- `POST /v1/chat/completions` - OpenAI protocol
- `POST /v1/messages` - Anthropic protocol
- `GET /health` - Health check

## Model Name Replacement

Claude Code sends placeholder model names that are automatically replaced:

- `code-default-model` → provider's configured model
- `code-haiku-model` → provider's configured model
- `code-opus-model` → provider's configured model
- `code-sonnet-model` → provider's configured model
- `code-fast-model` → provider's configured model

## License

MIT License
