import { invoke } from "@tauri-apps/api/core";

export interface Provider {
  id: number;
  name: string;
  remark: string;
  model: string;
  api_type: string;
  base_url: string;
  api_key: string;
}

export interface ProviderInput {
  name: string;
  remark: string;
  model: string;
  api_type: string;
  base_url: string;
  api_key: string;
}

export async function listProviders(): Promise<Provider[]> {
  return invoke("list_providers");
}

export async function getProvider(id: number): Promise<Provider | null> {
  return invoke("get_provider", { id });
}

export async function addProvider(provider: ProviderInput): Promise<number> {
  return invoke("add_provider", { provider });
}

export async function updateProvider(id: number, provider: ProviderInput): Promise<void> {
  return invoke("update_provider", { id, provider });
}

export async function deleteProvider(id: number): Promise<void> {
  return invoke("delete_provider", { id });
}

export async function startProxy(providerId: number): Promise<void> {
  return invoke("start_proxy", { providerId });
}

export async function stopProxy(): Promise<void> {
  return invoke("stop_proxy");
}

export async function getProxyStatus(): Promise<boolean> {
  return invoke("get_proxy_status");
}

export async function switchProxyProvider(providerId: number): Promise<void> {
  return invoke("switch_proxy_provider", { providerId });
}

export async function getCurrentProvider(): Promise<Provider | null> {
  return invoke("get_current_provider");
}

export interface SessionMeta {
  providerId: string;
  sessionId: string;
  title?: string;
  summary?: string;
  projectDir?: string;
  createdAt?: number;
  lastActiveAt?: number;
  sourcePath?: string;
  resumeCommand?: string;
}

export interface SessionMessage {
  role: string;
  content: string;
  ts?: number;
}

export async function listSessions(): Promise<SessionMeta[]> {
  return invoke("list_sessions");
}

export async function getSessionMessages(
  providerId: string,
  sourcePath: string
): Promise<SessionMessage[]> {
  return invoke("get_session_messages", { providerId, sourcePath });
}

export async function deleteSession(sourcePath: string): Promise<boolean> {
  return invoke("delete_session", { sourcePath });
}

export async function hideWindow(): Promise<void> {
  return invoke("hide_window");
}

export async function refreshTrayMenu(): Promise<void> {
  return invoke("refresh_tray_menu");
}

export async function getSystemLocale(): Promise<string> {
  return invoke("get_system_locale");
}

export async function getLanguage(): Promise<string | null> {
  return invoke("get_language");
}

export async function setLanguage(lang: string): Promise<void> {
  return invoke("set_language", { lang });
}
