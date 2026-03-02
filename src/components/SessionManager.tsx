import { useState, useEffect } from "react";
import { listSessions, getSessionMessages, deleteSession, SessionMeta, SessionMessage } from "../lib/api";
import { t } from "../lib/i18n";

interface SessionManagerProps {
  onClose: () => void;
}

function formatTimestamp(ts: number | undefined): string {
  if (!ts) return "";
  const date = new Date(ts);
  return date.toLocaleString();
}

function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + "...";
}

export function SessionManager({ onClose }: SessionManagerProps) {
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedSession, setSelectedSession] = useState<SessionMeta | null>(null);
  const [messages, setMessages] = useState<SessionMessage[]>([]);
  const [loadingMessages, setLoadingMessages] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<SessionMeta | null>(null);

  useEffect(() => {
    loadSessions();
  }, []);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        if (deleteConfirm) {
          setDeleteConfirm(null);
        } else {
          onClose();
        }
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [deleteConfirm, onClose]);

  async function loadSessions() {
    setLoading(true);
    try {
      const data = await listSessions();
      setSessions(data);
    } catch (e) {
      console.error("Failed to load sessions:", e);
    } finally {
      setLoading(false);
    }
  }

  async function loadMessages(session: SessionMeta) {
    if (!session.sourcePath) return;
    setSelectedSession(session);
    setLoadingMessages(true);
    try {
      const data = await getSessionMessages(session.providerId, session.sourcePath);
      setMessages(data);
    } catch (e) {
      console.error("Failed to load messages:", e);
      setMessages([]);
    } finally {
      setLoadingMessages(false);
    }
  }

  async function handleCopy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      console.error("Failed to copy:", e);
    }
  }

  async function handleDelete(session: SessionMeta, e: React.MouseEvent) {
    e.stopPropagation();
    if (!session.sourcePath) return;
    setDeleteConfirm(session);
  }

  async function confirmDelete() {
    if (!deleteConfirm) return;

    try {
      await deleteSession(deleteConfirm.sourcePath!);
      setSessions(sessions.filter(
        (s) => !(s.sessionId === deleteConfirm.sessionId && s.providerId === deleteConfirm.providerId)
      ));
      if (selectedSession?.sessionId === deleteConfirm.sessionId) {
        setSelectedSession(null);
        setMessages([]);
      }
    } catch (e) {
      console.error("Failed to delete session:", e);
      alert(`Failed to delete session: ${e}`);
    } finally {
      setDeleteConfirm(null);
    }
  }

  function cancelDelete() {
    setDeleteConfirm(null);
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-zinc-800 rounded-lg border border-zinc-700 w-[900px] h-[600px] flex flex-col relative">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-700">
          <h2 className="text-lg font-semibold">{t("session.title")}</h2>
          <button
            onClick={onClose}
            className="text-zinc-400 hover:text-zinc-200 text-xl"
          >
            &times;
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 flex overflow-hidden">
          {/* Session List */}
          <div className="w-[300px] border-r border-zinc-700 overflow-y-auto">
            <div className="p-2">
              {loading ? (
                <div className="text-center text-zinc-400 py-8">{t("session.loading")}</div>
              ) : sessions.length === 0 ? (
                <div className="text-center text-zinc-400 py-8">{t("session.noSessions")}</div>
              ) : (
                <div className="space-y-1">
                  {sessions.map((session) => (
                    <div
                      key={`${session.providerId}-${session.sessionId}`}
                      onClick={() => loadMessages(session)}
                      className={`p-2 rounded cursor-pointer group ${
                        selectedSession?.sessionId === session.sessionId
                          ? "bg-zinc-700"
                          : "hover:bg-zinc-700/50"
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <div className="font-medium text-sm truncate flex-1">
                          {session.title || session.sessionId}
                        </div>
                        <button
                          onClick={(e) => handleDelete(session, e)}
                          className="text-zinc-500 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-opacity ml-2"
                          title="Delete session"
                        >
                          ✕
                        </button>
                      </div>
                      {session.summary && (
                        <div className="text-xs text-zinc-400 truncate mt-1">
                          {session.summary}
                        </div>
                      )}
                      <div className="text-xs text-zinc-500 mt-1">
                        {session.lastActiveAt
                          ? formatTimestamp(session.lastActiveAt)
                          : session.createdAt
                          ? formatTimestamp(session.createdAt)
                          : ""}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>

          {/* Session Messages */}
          <div className="flex-1 flex flex-col overflow-hidden">
            {!selectedSession ? (
              <div className="flex-1 flex items-center justify-center text-zinc-400">
                {t("session.selectSession")}
              </div>
            ) : loadingMessages ? (
              <div className="flex-1 flex items-center justify-center text-zinc-400">
                {t("session.loadingMessages")}
              </div>
            ) : messages.length === 0 ? (
              <div className="flex-1 flex items-center justify-center text-zinc-400">
                {t("session.noMessages")}
              </div>
            ) : (
              <div className="flex-1 overflow-y-auto p-4 space-y-4">
                {messages.map((msg, idx) => (
                  <div
                    key={idx}
                    className={`p-3 rounded ${
                      msg.role === "user"
                        ? "bg-zinc-700/50 ml-8"
                        : "bg-zinc-700 mr-8"
                    }`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <span
                        className={`text-xs font-medium ${
                          msg.role === "user" ? "text-blue-400" : "text-green-400"
                        }`}
                      >
                        {msg.role}
                      </span>
                      <button
                        onClick={() => handleCopy(msg.content)}
                        className="text-xs text-zinc-500 hover:text-zinc-300"
                      >
                        {t("session.copy")}
                      </button>
                    </div>
                    <div className="text-sm whitespace-pre-wrap">
                      {truncateText(msg.content, 500)}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Delete Confirmation Dialog */}
        {deleteConfirm && (
          <div className="absolute inset-0 bg-black/50 flex items-center justify-center z-10">
            <div className="bg-zinc-800 rounded-lg border border-zinc-700 p-6 max-w-sm">
              <h3 className="text-lg font-semibold mb-4">{t("session.confirmDelete")}</h3>
              <p className="text-sm text-zinc-400 mb-6">
                {t("session.deleteMessage")}
              </p>
              <div className="flex justify-end gap-3">
                <button
                  onClick={cancelDelete}
                  className="px-4 py-2 rounded text-sm bg-zinc-700 hover:bg-zinc-600"
                >
                  {t("session.cancel")}
                </button>
                <button
                  onClick={confirmDelete}
                  className="px-4 py-2 rounded text-sm bg-red-600 hover:bg-red-700"
                >
                  {t("session.delete")}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
