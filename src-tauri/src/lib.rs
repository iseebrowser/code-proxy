// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod proxy;
mod config;
mod database;
mod provider;
mod mcp;
mod session_manager;

use std::sync::{Arc, Mutex};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};
use tokio::sync::RwLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    pub proxy_server: Arc<RwLock<Option<proxy::server::ProxyServer>>>,
    pub current_provider_id: Arc<RwLock<Option<i64>>>,
}

#[tauri::command]
async fn start_proxy(
    state: tauri::State<'_, AppState>,
    provider_id: i64,
) -> Result<(), String> {
    let provider = {
        let db = database::get_database()?;
        provider::get_provider(&db, provider_id)
            .map_err(|e| e.to_string())?
            .ok_or("Provider not found")?
    };

    let mut server_lock = state.proxy_server.write().await;
    if server_lock.is_some() {
        return Err("Proxy server already running".to_string());
    }

    let mut server = proxy::server::ProxyServer::new(provider);
    server.start().map_err(|e| e.to_string())?;

    // Update Claude config
    config::update_claude_config(true).map_err(|e| e.to_string())?;

    *server_lock = Some(server);

    // Save current provider to database
    {
        let db = database::get_database()?;
        database::set_setting(&db, "current_provider_id", &provider_id.to_string())
            .map_err(|e| e.to_string())?;
    }

    *state.current_provider_id.write().await = Some(provider_id);

    Ok(())
}

#[tauri::command]
async fn stop_proxy(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut server_lock = state.proxy_server.write().await;
    if let Some(mut server) = server_lock.take() {
        server.stop().await.map_err(|e| e.to_string())?;
    }

    // Restore Claude config
    config::update_claude_config(false).map_err(|e| e.to_string())?;

    // Clear current provider from database
    {
        let db = database::get_database()?;
        let _ = database::set_setting(&db, "current_provider_id", "");
    }

    *state.current_provider_id.write().await = None;

    Ok(())
}

#[tauri::command]
async fn get_proxy_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let server_lock = state.proxy_server.read().await;
    Ok(server_lock.is_some())
}

#[tauri::command]
async fn switch_proxy_provider(
    state: tauri::State<'_, AppState>,
    provider_id: i64,
) -> Result<(), String> {
    let provider = {
        let db = database::get_database()?;
        provider::get_provider(&db, provider_id)
            .map_err(|e| e.to_string())?
            .ok_or("Provider not found")?
    };

    let server_lock = state.proxy_server.read().await;
    if let Some(server) = server_lock.as_ref() {
        server.switch_provider(provider).await;
    }

    // Update saved provider ID
    {
        let db = database::get_database()?;
        database::set_setting(&db, "current_provider_id", &provider_id.to_string())
            .map_err(|e| e.to_string())?;
    }

    *state.current_provider_id.write().await = Some(provider_id);

    Ok(())
}

#[tauri::command]
fn list_providers() -> Result<Vec<provider::Provider>, String> {
    let db = database::get_database()?;
    provider::list_providers(&db).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_provider(id: i64) -> Result<Option<provider::Provider>, String> {
    let db = database::get_database()?;
    provider::get_provider(&db, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_provider(provider: provider::ProviderInput) -> Result<i64, String> {
    let db = database::get_database()?;
    provider::add_provider(&db, provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_provider(id: i64, provider: provider::ProviderInput) -> Result<(), String> {
    let db = database::get_database()?;
    provider::update_provider(&db, id, provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_provider(id: i64) -> Result<(), String> {
    let db = database::get_database()?;
    provider::delete_provider(&db, id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_current_provider(state: tauri::State<'_, AppState>) -> Result<Option<provider::Provider>, String> {
    let provider_id = state.current_provider_id.read().await;
    if let Some(id) = *provider_id {
        let provider = {
            let db = database::get_database()?;
            provider::get_provider(&db, id).map_err(|e| e.to_string())?
        };
        Ok(provider)
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn list_sessions() -> Result<Vec<session_manager::SessionMeta>, String> {
    let sessions = tauri::async_runtime::spawn_blocking(session_manager::scan_sessions)
        .await
        .map_err(|e| format!("Failed to scan sessions: {e}"))?;
    Ok(sessions)
}

#[tauri::command]
async fn get_session_messages(
    providerId: String,
    sourcePath: String,
) -> Result<Vec<session_manager::SessionMessage>, String> {
    let provider_id = providerId.clone();
    let source_path = sourcePath.clone();
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to load session messages: {e}"))?
}

#[tauri::command]
async fn delete_session(sourcePath: String) -> Result<bool, String> {
    let path = std::path::Path::new(&sourcePath);
    if !path.exists() {
        return Err("Session file not found".to_string());
    }

    std::fs::remove_file(path)
        .map_err(|e| format!("Failed to delete session file: {e}"))?;

    tracing::info!("Deleted session file: {}", sourcePath);
    Ok(true)
}

#[tauri::command]
fn hide_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_system_locale() -> String {
    // Get system UI language using Windows API via std
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        // Use systeminfo or PowerShell to get the system locale
        let output = Command::new("powershell")
            .args(["-Command", "(Get-Culture).Name"])
            .output();

        if let Ok(output) = output {
            let lang = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if lang.starts_with("zh") {
                return "zh-CN".to_string();
            }
        }

        "en-US".to_string()
    }

    #[cfg(not(target_os = "windows"))]
    {
        "en-US".to_string()
    }
}

#[tauri::command]
fn get_language() -> Result<Option<String>, String> {
    let db = database::get_database()?;
    database::get_setting(&db, "language").map_err(|e| e.to_string())
}

#[tauri::command]
fn set_language(lang: String) -> Result<(), String> {
    let db = database::get_database()?;
    database::set_setting(&db, "language", &lang).map_err(|e| e.to_string())
}

#[tauri::command]
fn refresh_tray_menu(app: AppHandle) -> Result<(), String> {
    // Get the tray icon and rebuild menu
    if let Some(tray) = app.tray_by_id("main") {
        let new_menu = build_tray_menu(&app).map_err(|e| e.to_string())?;
        tray.set_menu(Some(new_menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn get_locale_string(key: &str) -> String {
    // Get saved language or detect system language
    let lang = if let Ok(conn) = database::get_database() {
        database::get_setting(&conn, "language")
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                // First launch: detect and save
                let detected = get_system_locale();
                let _ = database::set_setting(&conn, "language", &detected);
                detected
            })
    } else {
        get_system_locale()
    };

    // Return localized strings
    match key {
        "open" => {
            if lang.starts_with("zh") {
                "打开主界面".to_string()
            } else {
                "Show Main Window".to_string()
            }
        }
        "quit" => {
            if lang.starts_with("zh") {
                "退出".to_string()
            } else {
                "Quit".to_string()
            }
        }
        _ => key.to_string(),
    }
}

pub fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let providers = provider::list_providers(&database::get_database().unwrap()).unwrap_or_default();

    let current_provider_id: Option<i64> = {
        if let Ok(conn) = database::get_database() {
            database::get_setting(&conn, "current_provider_id")
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
        } else {
            None
        }
    };

    let menu = Menu::new(app)?;

    // Get localized "open" string
    let open_text = get_locale_string("open");
    let open_item = MenuItem::with_id(app, "open", &open_text, true, None::<&str>)?;
    menu.append(&open_item)?;

    // Separator
    let sep1 = PredefinedMenuItem::separator(app)?;
    menu.append(&sep1)?;

    // Provider items
    for provider in &providers {
        let checked = current_provider_id == Some(provider.id);
        let item = CheckMenuItem::with_id(
            app,
            format!("provider_{}", provider.id),
            &provider.name,
            true,
            checked,
            None::<&str>,
        )?;
        menu.append(&item)?;
    }

    // Separator
    let sep2 = PredefinedMenuItem::separator(app)?;
    menu.append(&sep2)?;

    // Get localized "quit" string
    let quit_text = get_locale_string("quit");
    let quit_item = MenuItem::with_id(app, "quit", &quit_text, true, None::<&str>)?;
    menu.append(&quit_item)?;

    Ok(menu)
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .init();

    // Initialize database
    if let Err(e) = database::init_database() {
        tracing::error!("Failed to initialize database: {}", e);
    }

    // Load saved current provider
    let saved_provider_id = {
        if let Ok(conn) = database::get_database() {
            database::get_setting(&conn, "current_provider_id")
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
        } else {
            None
        }
    };

    let app_state = AppState {
        proxy_server: Arc::new(RwLock::new(None)),
        current_provider_id: Arc::new(RwLock::new(None)),
    };

    // Auto-start proxy if there's a saved provider
    let auto_start_provider_id = saved_provider_id;
    let proxy_server_for_auto_start = app_state.proxy_server.clone();
    let current_provider_id_for_auto_start = app_state.current_provider_id.clone();

    // Spawn MCP server - it shares proxy state with AppState
    let mcp_proxy_state = app_state.proxy_server.clone();
    let mcp_current_provider = app_state.current_provider_id.clone();
    let mcp_merged_state = Arc::new(mcp::server::McpState {
        proxy_server: mcp_proxy_state,
        current_provider_id: mcp_current_provider,
        app_handle: Arc::new(Mutex::new(None)),
    });
    let mcp_state_for_setup = mcp_merged_state.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime for MCP");
        rt.block_on(async {
            // Auto-start proxy if there's a saved provider
            if let Some(provider_id) = auto_start_provider_id {
                tracing::info!("Auto-starting proxy with provider id: {}", provider_id);
                let provider = {
                    if let Ok(conn) = database::get_database() {
                        provider::get_provider(&conn, provider_id).ok().flatten()
                    } else {
                        None
                    }
                };

                if let Some(provider) = provider {
                    let mut server = proxy::server::ProxyServer::new(provider);
                    if let Err(e) = server.start() {
                        tracing::error!("Failed to auto-start proxy: {}", e);
                    } else {
                        // Update Claude config
                        if let Err(e) = config::update_claude_config(true) {
                            tracing::error!("Failed to update Claude config: {}", e);
                        }
                        *proxy_server_for_auto_start.write().await = Some(server);
                        *current_provider_id_for_auto_start.write().await = Some(provider_id);
                        tracing::info!("Proxy auto-started successfully");
                    }
                }
            }

            if let Err(e) = mcp::server::run_mcp_server(mcp_merged_state).await {
                tracing::error!("MCP server error: {}", e);
            }
        });
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // When a second instance is started, hide the main window
            tracing::info!("Second instance detected, hiding main window");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Update MCP state with AppHandle for event emission
            {
                let mut state = mcp_state_for_setup.app_handle.lock().unwrap();
                *state = Some(app_handle.clone());
            }

            // Build tray menu
            let menu = build_tray_menu(&app_handle)?;

            // Create tray icon
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } = event {
                        // Double click - show window
                        show_window(tray.app_handle());
                    }
                })
                .on_menu_event(move |app, event| {
                    let id = event.id().as_ref();
                    match id {
                        "open" => {
                            show_window(app);
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        id if id.starts_with("provider_") => {
                            if let Ok(provider_id) = id.replace("provider_", "").parse::<i64>() {
                                // First, save the new provider_id to database and state BEFORE rebuilding menu
                                // Use blocking write since we're not in async context
                                {
                                    let state = app.state::<AppState>();
                                    let current = state.current_provider_id.clone();
                                    let mut guard = current.blocking_write();
                                    *guard = Some(provider_id);
                                }
                                {
                                    let db = database::get_database();
                                    if let Ok(conn) = db {
                                        let _ = database::set_setting(&conn, "current_provider_id", &provider_id.to_string());
                                    }
                                }

                                // Rebuild menu with updated provider
                                if let Some(tray) = app.tray_by_id("main") {
                                    if let Ok(new_menu) = build_tray_menu(&app) {
                                        let _ = tray.set_menu(Some(new_menu));
                                    }
                                }

                                // Clone app handle for async task
                                let app_handle = app.clone();
                                // Get state from app
                                let state = app.state::<AppState>();
                                let state_clone = state.proxy_server.clone();

                                // Spawn async task to switch provider in proxy
                                tauri::async_runtime::spawn(async move {
                                    // Get provider info
                                    let provider = {
                                        let db = database::get_database().unwrap();
                                        provider::get_provider(&db, provider_id).ok().flatten()
                                    };

                                    if let Some(provider) = provider {
                                        // Switch provider in proxy server if running
                                        {
                                            let server_lock = state_clone.read().await;
                                            if let Some(server) = server_lock.as_ref() {
                                                server.switch_provider(provider).await;
                                            }
                                        }

                                        // Emit event to frontend
                                        let _ = app_handle.emit("provider-changed", provider_id);
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide window instead of closing
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_proxy,
            stop_proxy,
            get_proxy_status,
            switch_proxy_provider,
            list_providers,
            get_provider,
            add_provider,
            update_provider,
            delete_provider,
            get_current_provider,
            list_sessions,
            get_session_messages,
            delete_session,
            hide_window,
            refresh_tray_menu,
            get_system_locale,
            get_language,
            set_language,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
