import { useState, useEffect, useRef } from "react";
import { Provider, listProviders, deleteProvider, addProvider, updateProvider, switchProxyProvider, refreshTrayMenu, ProviderInput } from "../lib/api";
import { cn } from "../lib/utils";
import { t } from "../lib/i18n";

interface ProviderSelectorProps {
  selectedProvider: Provider | null;
  onSelect: (provider: Provider) => void;
  isRunning?: boolean;
  disabled?: boolean;
}

export function ProviderSelector({ selectedProvider, onSelect, isRunning, disabled }: ProviderSelectorProps) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [isOpen, setIsOpen] = useState(false);
  const [isAdding, setIsAdding] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [newProvider, setNewProvider] = useState<ProviderInput>({
    name: "",
    remark: "",
    model: "",
    api_type: "anthropic",
    base_url: "",
    api_key: "",
  });
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadProviders();
  }, []);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape" && (isAdding || isEditing)) {
        closeDialog();
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isAdding, isEditing]);

  async function loadProviders() {
    try {
      const list = await listProviders();
      setProviders(list);
    } catch (e) {
      console.error("Failed to load providers:", e);
    }
  }

  async function handleDelete(id: number) {
    try {
      await deleteProvider(id);
      loadProviders();
      if (selectedProvider?.id === id) {
        onSelect(null as any);
      }
      await refreshTrayMenu();
      setIsDeleting(false);
      setDeletingId(null);
    } catch (e) {
      console.error("Failed to delete provider:", e);
    }
  }

  function confirmDelete(id: number) {
    setDeletingId(id);
    setIsDeleting(true);
  }

  async function handleAdd() {
    if (!newProvider.model || !newProvider.base_url || !newProvider.api_key) return;
    try {
      const providerToSave = {
        ...newProvider,
        name: newProvider.model,
      };
      await addProvider(providerToSave);
      setIsAdding(false);
      setNewProvider({
        name: "",
        remark: "",
        model: "",
        api_type: "anthropic",
        base_url: "",
        api_key: "",
      });
      loadProviders();
      await refreshTrayMenu();
    } catch (e) {
      console.error("Failed to add provider:", e);
    }
  }

  function handleEdit(provider: Provider) {
    setEditingId(provider.id);
    setNewProvider({
      name: provider.name,
      remark: provider.remark,
      model: provider.model,
      api_type: provider.api_type === "openai" ? "openai_chat" : provider.api_type,
      base_url: provider.base_url,
      api_key: provider.api_key,
    });
    setIsEditing(true);
  }

  async function handleModify() {
    if (!editingId || !newProvider.model || !newProvider.base_url || !newProvider.api_key) return;
    try {
      const providerToSave = {
        ...newProvider,
        name: newProvider.model,
      };
      await updateProvider(editingId, providerToSave);
      setIsEditing(false);
      setEditingId(null);
      setNewProvider({
        name: "",
        remark: "",
        model: "",
        api_type: "anthropic",
        base_url: "",
        api_key: "",
      });
      loadProviders();
      await refreshTrayMenu();
    } catch (e) {
      console.error("Failed to modify provider:", e);
    }
  }

  function closeDialog() {
    setIsAdding(false);
    setIsEditing(false);
    setIsDeleting(false);
    setEditingId(null);
    setDeletingId(null);
    setNewProvider({
      name: "",
      remark: "",
      model: "",
      api_type: "anthropic",
      base_url: "",
      api_key: "",
    });
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="relative" ref={dropdownRef}>
        <button
          type="button"
          onClick={() => !disabled && setIsOpen(!isOpen)}
          className={cn(
            "inline-flex items-center justify-between rounded-md px-4 py-2 text-sm w-48",
            "bg-zinc-800 text-zinc-100 border border-zinc-700",
            "hover:bg-zinc-700 focus:outline-none focus:ring-2 focus:ring-zinc-500",
            "disabled:opacity-50 disabled:cursor-not-allowed"
          )}
        >
          <span>{selectedProvider?.name || t("provider.selectProvider")}</span>
          <span className="ml-2">▼</span>
        </button>

        {isOpen && (
          <div className="absolute top-full left-0 mt-1 w-56 bg-zinc-800 border border-zinc-700 rounded-md shadow-lg z-50">
            {providers.map((provider) => (
              <div
                key={provider.id}
                className={cn(
                  "flex items-center justify-between px-3 py-2 text-sm cursor-pointer",
                  "text-zinc-100 hover:bg-zinc-700",
                  selectedProvider?.id === provider.id && "bg-zinc-700"
                )}
                onClick={async () => {
                  if (isRunning && selectedProvider?.id !== provider.id) {
                    // Switch provider without restarting proxy
                    try {
                      await switchProxyProvider(provider.id);
                    } catch (e) {
                      console.error("Failed to switch provider:", e);
                    }
                  }
                  onSelect(provider);
                  setIsOpen(false);
                  await refreshTrayMenu();
                }}
              >
                <span>{provider.name}</span>
                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleEdit(provider);
                      setIsOpen(false);
                    }}
                    disabled={selectedProvider?.id === provider.id}
                    className="p-1 text-zinc-400 hover:text-zinc-100 disabled:opacity-30 disabled:cursor-not-allowed"
                    title="Edit"
                  >
                    ✏️
                  </button>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      confirmDelete(provider.id);
                    }}
                    disabled={selectedProvider?.id === provider.id}
                    className="p-1 text-zinc-400 hover:text-red-400 disabled:opacity-30 disabled:cursor-not-allowed"
                    title="Delete"
                  >
                    ×
                  </button>
                </div>
              </div>
            ))}
            <div
              className="px-3 py-2 text-sm text-zinc-400 cursor-pointer hover:bg-zinc-700"
              onClick={() => {
                setIsAdding(true);
                setIsOpen(false);
              }}
            >
              + {t("provider.addProvider")}
            </div>
          </div>
        )}
      </div>

      {/* Dialog */}
      {(isAdding || isEditing) && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-[100]">
          <div className="bg-zinc-800 p-6 rounded-lg w-[480px] border border-zinc-700">
            <h3 className="text-lg font-semibold text-zinc-100 mb-4">
              {isEditing ? t("provider.editProvider") : t("provider.addProvider")}
            </h3>
            <div className="space-y-3">
              {/* Row 1: Provider, Remark */}
              <div className="flex gap-3">
                <div className="flex-1">
                  <label className="block text-sm text-zinc-400 mb-1">{t("provider.name")}</label>
                  <input
                    type="text"
                    placeholder="ANTHROPIC_MODEL"
                    value={newProvider.model}
                    onChange={(e) => setNewProvider({ ...newProvider, model: e.target.value })}
                    className="w-full px-3 py-2 bg-zinc-900 border border-zinc-700 rounded text-zinc-100"
                  />
                </div>
                <div className="flex-1">
                  <label className="block text-sm text-zinc-400 mb-1">{t("provider.remark")}</label>
                  <input
                    type="text"
                    placeholder={t("provider.remark")}
                    value={newProvider.remark}
                    onChange={(e) => setNewProvider({ ...newProvider, remark: e.target.value })}
                    className="w-full px-3 py-2 bg-zinc-900 border border-zinc-700 rounded text-zinc-100"
                  />
                </div>
              </div>

              {/* Row 2: API Key */}
              <div>
                <label className="block text-sm text-zinc-400 mb-1">{t("provider.apiKey")} ({t("provider.apiKeyPlaceholder")})</label>
                <input
                  type="password"
                  placeholder={t("provider.apiKey")}
                  value={newProvider.api_key}
                  onChange={(e) => setNewProvider({ ...newProvider, api_key: e.target.value })}
                  className="w-full px-3 py-2 bg-zinc-900 border border-zinc-700 rounded text-zinc-100"
                />
              </div>

              {/* Row 3: Base URL */}
              <div>
                <label className="block text-sm text-zinc-400 mb-1">{t("provider.baseUrl")} ({t("provider.baseUrlPlaceholder")})</label>
                <input
                  type="text"
                  placeholder="https://api.anthropic.com"
                  value={newProvider.base_url}
                  onChange={(e) => setNewProvider({ ...newProvider, base_url: e.target.value })}
                  className="w-full px-3 py-2 bg-zinc-900 border border-zinc-700 rounded text-zinc-100"
                />
              </div>

              {/* Row 4: API Type */}
              <div>
                <label className="block text-sm text-zinc-400 mb-1">{t("provider.apiType")}</label>
                <select
                  value={newProvider.api_type}
                  onChange={(e) => setNewProvider({ ...newProvider, api_type: e.target.value })}
                  className="w-full px-3 py-2 bg-zinc-900 border border-zinc-700 rounded text-zinc-100"
                >
                  <option value="anthropic">{t("provider.anthropicMessages")}</option>
                  <option value="openai_chat">{t("provider.openAIChat")}</option>
                  <option value="openai_responses">OpenAI Responses API</option>
                </select>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <button
                type="button"
                onClick={closeDialog}
                className="px-4 py-2 text-zinc-400 hover:text-zinc-100"
              >
                {t("provider.cancel")}
              </button>
              <button
                type="button"
                onClick={isEditing ? handleModify : handleAdd}
                className="px-4 py-2 bg-zinc-600 text-zinc-100 rounded hover:bg-zinc-500"
              >
                {isEditing ? t("provider.modify") : t("provider.add")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete Confirmation Dialog */}
      {isDeleting && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-[100]">
          <div className="bg-zinc-800 p-6 rounded-lg w-[400px] border border-zinc-700">
            <h3 className="text-lg font-semibold text-zinc-100 mb-4">
              {t("provider.confirmDelete")}
            </h3>
            <p className="text-zinc-400 mb-6">
              {t("provider.deleteMessage")}
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={closeDialog}
                className="px-4 py-2 text-zinc-400 hover:text-zinc-100"
              >
                {t("provider.cancel")}
              </button>
              <button
                type="button"
                onClick={() => deletingId && handleDelete(deletingId)}
                className="px-4 py-2 bg-red-600 text-zinc-100 rounded hover:bg-red-500"
              >
                {t("provider.delete")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
