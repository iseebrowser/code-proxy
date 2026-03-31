import { useState, useEffect, useRef } from "react";
import { ProviderSelector } from "./components/ProviderSelector";
import { ProxyToggle } from "./components/ProxyToggle";
import { SessionManager } from "./components/SessionManager";
import { Provider, getCurrentProvider, getProxyStatus } from "./lib/api";
import { listen } from "@tauri-apps/api/event";
import { initI18n, t, changeLanguage, getCurrentLanguage, isTranslationsLoaded } from "./lib/i18n";

function App() {
  const [selectedProvider, setSelectedProvider] = useState<Provider | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [showSessionManager, setShowSessionManager] = useState(false);
  const [showLangMenu, setShowLangMenu] = useState(false);
  const [lang, setLang] = useState<"en-US" | "zh-CN">("en-US");
  const [ready, setReady] = useState(false);
  const langMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Initialize i18n first
    initI18n().then(() => {
      setLang(getCurrentLanguage());
      setReady(true);
    });

    loadCurrentProvider();
    checkStatus();

    // Poll status a few times to catch auto-started proxy
    const intervals = [100, 500, 1000, 2000].map((delay) =>
      setTimeout(checkStatus, delay)
    );

    // Listen for provider-changed event from system tray
    const unlisten = listen<number>("provider-changed", async () => {
      const provider = await getCurrentProvider();
      if (provider) {
        setSelectedProvider(provider);
      }
    });

    return () => {
      intervals.forEach(clearTimeout);
      unlisten.then((fn) => fn());
    };
  }, []);

  // Close language menu on click outside or ESC key
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (langMenuRef.current && !langMenuRef.current.contains(event.target as Node)) {
        setShowLangMenu(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setShowLangMenu(false);
      }
    }

    if (showLangMenu) {
      document.addEventListener("mousedown", handleClickOutside);
      document.addEventListener("keydown", handleKeyDown);
    }

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [showLangMenu]);

  async function loadCurrentProvider() {
    try {
      const provider = await getCurrentProvider();
      if (provider) {
        setSelectedProvider(provider);
      }
    } catch (e) {
      console.error("Failed to load current provider:", e);
    }
  }

  async function checkStatus() {
    try {
      const status = await getProxyStatus();
      setIsRunning(status);
    } catch (e) {
      console.error("Failed to check proxy status:", e);
    }
  }

  async function handleLanguageChange(newLang: "en-US" | "zh-CN") {
    await changeLanguage(newLang);
    setLang(newLang);
    setShowLangMenu(false);
    // Force re-render by updating state
    setReady(false);
    setReady(true);
  }

  if (!ready || !isTranslationsLoaded()) {
    return (
      <div className="min-h-screen bg-zinc-900 text-zinc-100 flex items-center justify-center">
        <div className="text-zinc-400">{t("proxy.loading")}</div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-zinc-900 text-zinc-100">
      <div className="max-w-4xl mx-auto p-6">
        <h1 className="text-2xl font-bold mb-6">{t("app.title")}</h1>

        <div className="flex items-center gap-4 mb-8">
          <ProviderSelector
            selectedProvider={selectedProvider}
            onSelect={setSelectedProvider}
            isRunning={isRunning}
          />
          <ProxyToggle
            provider={selectedProvider}
            isRunning={isRunning}
            onStatusChange={setIsRunning}
          />
          <button
            onClick={() => setShowSessionManager(true)}
            className="px-4 py-2 rounded-md font-medium text-sm transition-colors bg-zinc-700 hover:bg-zinc-600 text-white"
          >
            {t("toolbar.manageSessions")}
          </button>

          {/* Language Selector */}
          <div className="relative ml-auto" ref={langMenuRef}>
            <button
              onClick={() => setShowLangMenu(!showLangMenu)}
              className="px-4 py-2 rounded-md font-medium text-sm transition-colors bg-zinc-700 hover:bg-zinc-600 text-white"
              title={lang === "zh-CN" ? "中文" : "English"}
            >
              ⚙️
            </button>
            {showLangMenu && (
              <div className="absolute right-0 mt-2 w-32 bg-zinc-700 rounded-md shadow-lg z-10">
                <button
                  onClick={() => handleLanguageChange("en-US")}
                  className={`block w-full text-left px-4 py-2 text-sm hover:bg-zinc-600 rounded-t-md ${
                    lang === "en-US" ? "text-blue-400" : "text-white"
                  }`}
                >
                  {t("languages.en")}
                </button>
                <button
                  onClick={() => handleLanguageChange("zh-CN")}
                  className={`block w-full text-left px-4 py-2 text-sm hover:bg-zinc-600 rounded-b-md ${
                    lang === "zh-CN" ? "text-blue-400" : "text-white"
                  }`}
                >
                  {t("languages.zh")}
                </button>
              </div>
            )}
          </div>
        </div>

        <div className="bg-zinc-800 rounded-lg p-6 border border-zinc-700">
          <h2 className="text-lg font-semibold mb-4">{t("status.title")}</h2>
          <div className="space-y-2 text-sm text-zinc-400">
            <p>
              <span className="text-zinc-300">{t("status.provider")}:</span>{" "}
              {selectedProvider?.name || t("status.noneSelected")}
            </p>
            <p>
              <span className="text-zinc-300">{t("status.type")}:</span>{" "}
              {selectedProvider?.api_type || "-"}
            </p>
            <p>
              <span className="text-zinc-300">{t("status.baseUrl")}:</span>{" "}
              {selectedProvider?.base_url || "-"}
            </p>
            <p>
              <span className="text-zinc-300">{t("status.proxyPort")}:</span> 13721
            </p>
          </div>
        </div>

        <div className="mt-6 bg-zinc-800 rounded-lg p-6 border border-zinc-700">
          <h2 className="text-lg font-semibold mb-4">{t("howItWorks.title")}</h2>
          <p className="text-sm text-zinc-400 mb-4">
            {t("howItWorks.description")}
          </p>
          <div className="text-sm text-zinc-400">
            <p className="mb-2">{t("howItWorks.configuredToUse")}</p>
            <ul className="list-disc list-inside space-y-1">
              <li>ANTHROPIC_BASE_URL: http://127.0.0.1:13721</li>
              <li>API_TIMEOUT_MS: 7200000</li>
              <li>CLAUDE_CODE_MAX_OUTPUT_TOKENS: 131072</li>
            </ul>
          </div>
        </div>

        {showSessionManager && (
          <SessionManager onClose={() => setShowSessionManager(false)} />
        )}
      </div>
    </div>
  );
}

export default App;
