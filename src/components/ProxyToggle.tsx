import { useState } from "react";
import { Provider, startProxy, stopProxy, hideWindow, refreshTrayMenu } from "../lib/api";
import { t } from "../lib/i18n";

interface ProxyToggleProps {
  provider: Provider | null;
  isRunning?: boolean;
  onStatusChange?: (running: boolean) => void;
}

export function ProxyToggle({ provider, isRunning = false, onStatusChange }: ProxyToggleProps) {
  const [isLoading, setIsLoading] = useState(false);

  async function handleToggle() {
    if (!provider) return;
    setIsLoading(true);
    try {
      if (isRunning) {
        await stopProxy();
        onStatusChange?.(false);
      } else {
        await startProxy(provider.id);
        onStatusChange?.(true);
        // Refresh tray menu to update checkmark
        await refreshTrayMenu();
        // Hide window after starting proxy
        await hideWindow();
      }
    } catch (e) {
      console.error("Failed to toggle proxy:", e);
      alert(`Failed to ${isRunning ? "stop" : "start"} proxy: ${e}`);
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <button
      onClick={handleToggle}
      disabled={!provider || isLoading}
      className={`
        px-4 py-2 rounded-md font-medium text-sm transition-colors
        ${
          isRunning
            ? "bg-red-600 hover:bg-red-700 text-white"
            : "bg-green-600 hover:bg-green-700 text-white"
        }
        disabled:opacity-50 disabled:cursor-not-allowed
      `}
    >
      {isLoading
        ? t("proxy.loading")
        : isRunning
        ? t("proxy.stopProxy")
        : t("proxy.startProxy")}
    </button>
  );
}
