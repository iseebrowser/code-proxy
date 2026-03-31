# AGENTS.md

This file provides guidance to coding agents working in this repository.

## Project Overview

Code Proxy is a Tauri desktop application that proxies and transforms requests between Anthropic and OpenAI-compatible APIs for Claude Code style clients.

- GUI: Tauri 2 + React 19 + TypeScript
- Proxy server: Axum on `127.0.0.1:13721`
- MCP server: integrated on `127.0.0.1:13722`
- Storage: SQLite provider database

## Build Commands

```bash
npm run tauri dev
npm run tauri build
npm run build
cargo build --release --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

## Architecture

### Frontend

- `src/App.tsx` - main shell UI
- `src/components/ProviderSelector.tsx` - provider CRUD and provider type selection
- `src/lib/api.ts` - Tauri IPC bindings
- `src/lib/i18n/` and `src/locales/` - localization

### Backend

- `src-tauri/src/lib.rs` - Tauri commands, app bootstrap, tray integration, shared state
- `src-tauri/src/proxy/server.rs` - Axum route registration and proxy server lifecycle
- `src-tauri/src/proxy/handlers.rs` - request parsing, protocol routing, upstream forwarding, Claude Code style headers
- `src-tauri/src/proxy/transform.rs` - Anthropic <-> OpenAI Chat Completions transforms
- `src-tauri/src/proxy/transform_responses.rs` - Anthropic <-> OpenAI Responses transforms
- `src-tauri/src/provider.rs` - provider model and CRUD
- `src-tauri/src/database.rs` - SQLite setup and settings table
- `src-tauri/src/config.rs` - Claude Code settings integration
- `src-tauri/src/mcp/` - MCP server

## Provider Types

Supported provider `api_type` values:

- `anthropic`
- `openai_chat`
- `openai_responses`

Legacy `openai` values should be treated as `openai_chat`.

## Proxy Behavior

The proxy must preserve these paths:

- Model side `POST /v1/messages` -> terminal side `Anthropic Messages`, `OpenAI Chat`, or `OpenAI Responses`
- Model side `POST /v1/chat/completions` -> terminal side `Anthropic Messages` or `OpenAI Chat`
- Model side `POST /v1/responses` -> terminal side `Anthropic Messages` or `OpenAI Responses`

Currently implemented non-streaming conversions:

- `OpenAI Chat -> Anthropic Messages`
- `OpenAI Responses -> Anthropic Messages`
- `Anthropic Messages -> OpenAI Chat`
- `Anthropic Messages -> OpenAI Responses`
- `OpenAI Chat -> OpenAI Chat` pass-through
- `OpenAI Responses -> OpenAI Responses` pass-through
- `Anthropic Messages -> Anthropic Messages` pass-through

## Model Handling

Claude Code placeholder models such as:

- `code-default-model`
- `code-haiku-model`
- `code-sonnet-model`
- `code-opus-model`
- `code-fast-model`

must be replaced with the configured provider model before forwarding upstream.

## Agent Expectations

- Prefer targeted edits and keep protocol logic in `src-tauri/src/proxy/`.
- When adding protocol support, update both request and response transforms.
- Keep Claude Code header emulation intact in `handlers.rs`.
- Add Rust unit tests for every new transform or protocol mapping.
- Run `cargo test --manifest-path src-tauri/Cargo.toml` after backend changes.
- Run `npm run build` after frontend or shared API shape changes.

## MCP Configuration

Claude Code MCP example:

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
