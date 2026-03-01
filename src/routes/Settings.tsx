import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { LLM_PROVIDERS, isProviderId } from "../constants/llm";
import { useModelsConfigStore } from "../stores/modelsConfig";
import { saveSetting, clearAllChatHistory, getMcpConfig, saveMcpConfig, testMcpServer } from "../lib/tauri";
import type { McpConfig, McpServerEntry } from "../lib/tauri";
import { useSettingsStore } from "../stores/settings";
import type { ToolInfo } from "../stores/settings";

type SectionId = "ai" | "governance" | "data" | "local-tools" | "mcp";

const iconClass = "w-[18px] h-[18px] shrink-0";

/** Mask API key for display after save: show prefix + suffix only to avoid exposing full secret. */
function maskApiKey(key: string): string {
  const t = key.trim();
  if (t.length === 0) return "";
  if (t.length <= 12) return "••••••••••••";
  return `${t.slice(0, 8)}...${t.slice(-4)}`;
}

function IconAI({ className }: { className?: string }) {
  return (
    <svg className={className ?? iconClass} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
      <rect x="4" y="4" width="16" height="16" rx="2" ry="2" />
      <line x1="9" y1="9" x2="15" y2="9" />
      <line x1="9" y1="13" x2="15" y2="13" />
      <line x1="9" y1="17" x2="12" y2="17" />
    </svg>
  );
}

function IconShield({ className }: { className?: string }) {
  return (
    <svg className={className ?? iconClass} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  );
}

function IconDatabase({ className }: { className?: string }) {
  return (
    <svg className={className ?? iconClass} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
      <ellipse cx="12" cy="5" rx="9" ry="3" />
      <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" />
      <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" />
    </svg>
  );
}

function IconWrench({ className }: { className?: string }) {
  return (
    <svg className={className ?? iconClass} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
      <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
    </svg>
  );
}

function IconPlug({ className }: { className?: string }) {
  return (
    <svg className={className ?? iconClass} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 22v-5" />
      <path d="M9 8V2" />
      <path d="M15 8V2" />
      <path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z" />
    </svg>
  );
}

function LocalToolsSection({ tools }: { tools: ToolInfo[] }) {
  const [activeOpen, setActiveOpen] = useState(true);
  const [notFoundOpen, setNotFoundOpen] = useState(true);
  const activeTools = tools.filter((t) => t.available);
  const notFoundTools = tools.filter((t) => !t.available);

  const CollapsibleHeader = ({
    label,
    count,
    open,
    onToggle,
  }: {
    label: string;
    count?: number;
    open: boolean;
    onToggle: () => void;
  }) => (
    <button
      type="button"
      onClick={onToggle}
      className="flex items-center gap-2 w-full text-left py-2.5 px-4 border-b border-[var(--border)] bg-[var(--surface-2)] hover:bg-[var(--surface-3)] transition-colors"
    >
      <svg
        className={`w-4 h-4 shrink-0 text-[var(--text-muted)] transition-transform ${open ? "rotate-90" : ""}`}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <polyline points="9 18 15 12 9 6" />
      </svg>
      <span className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)]">
        {label}
        {count !== undefined && (
          <span className="ml-1.5 font-sans normal-case font-semibold text-[var(--text)]">
            ({count})
          </span>
        )}
      </span>
    </button>
  );

  return (
    <div className="mb-8">
      <div className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)] mb-3 pb-2 border-b border-[var(--border)]">
        Detected local tools
      </div>
      <div className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] overflow-hidden">
        <div className="flex flex-col">
          <CollapsibleHeader
            label="Active"
            count={activeTools.length}
            open={activeOpen}
            onToggle={() => setActiveOpen((o) => !o)}
          />
          {activeOpen && (
            <div className="flex flex-col">
              {activeTools.length === 0 ? (
                <div className="py-3 px-4 text-[12px] text-[var(--text-muted)] font-mono">
                  No active tools
                </div>
              ) : (
                activeTools.map((t) => (
                  <div
                    key={t.name}
                    className="flex items-center gap-3 py-2.5 px-4 pl-8 border-b border-[var(--border)] last:border-b-0 hover:bg-[var(--surface-2)] transition-colors"
                  >
                    <span className="w-[7px] h-[7px] rounded-full flex-shrink-0 bg-[var(--accent)] shadow-[0_0_5px_var(--accent)]" />
                    <span className="font-mono text-[12px] font-medium text-[var(--text)] w-[90px] flex-shrink-0">
                      {t.name}
                    </span>
                    <span className="font-mono text-[11px] text-[var(--text-muted)] flex-1 truncate">
                      {t.detected_path || "—"}
                    </span>
                  </div>
                ))
              )}
            </div>
          )}
          <CollapsibleHeader
            label="Not found"
            count={notFoundTools.length}
            open={notFoundOpen}
            onToggle={() => setNotFoundOpen((o) => !o)}
          />
          {notFoundOpen && (
            <div className="flex flex-col">
              {notFoundTools.length === 0 ? (
                <div className="py-3 px-4 pl-8 text-[12px] text-[var(--text-muted)] font-mono">
                  All tools are available
                </div>
              ) : (
                notFoundTools.map((t) => (
                  <div
                    key={t.name}
                    className="flex items-center gap-3 py-2.5 px-4 pl-8 border-b border-[var(--border)] last:border-b-0 hover:bg-[var(--surface-2)] transition-colors"
                  >
                    <span className="w-[7px] h-[7px] rounded-full flex-shrink-0 bg-[var(--text-dim)]" />
                    <span className="font-mono text-[12px] font-medium text-[var(--text)] w-[90px] flex-shrink-0">
                      {t.name}
                    </span>
                    <span className="font-mono text-[11px] text-[var(--text-muted)] flex-1 truncate">
                      {t.detected_path || "not found"}
                    </span>
                    <span className="font-mono text-[10px] px-1.5 py-0.5 rounded border font-medium bg-[var(--accent-dim)] text-[var(--accent)] border-[rgba(0,255,136,0.2)] shrink-0 cursor-default">
                      Install
                    </span>
                  </div>
                ))
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

const SECTIONS: { id: SectionId; label: string; Icon: React.ComponentType<{ className?: string }> }[] = [
  { id: "ai", label: "AI Provider", Icon: IconAI },
  { id: "governance", label: "Governance", Icon: IconShield },
  { id: "data", label: "Data & Storage", Icon: IconDatabase },
  { id: "local-tools", label: "Local Tools", Icon: IconWrench },
  { id: "mcp", label: "MCP Servers", Icon: IconPlug },
];

function McpServersSection({ appDataDir }: { appDataDir: string }) {
  const [config, setConfig] = useState<McpConfig>({ mcpServers: {} });
  const [loading, setLoading] = useState(true);
  const [saveFeedback, setSaveFeedback] = useState(false);
  const [saving, setSaving] = useState(false);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [formName, setFormName] = useState("");
  const [formCommand, setFormCommand] = useState("");
  const [formArgs, setFormArgs] = useState("");
  const [testResult, setTestResult] = useState<{ key: string; count?: number; error?: string } | null>(null);

  const load = async () => {
    setLoading(true);
    try {
      const c = await getMcpConfig();
      setConfig(c);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const startAdd = () => {
    setEditingKey("__new__");
    setFormName("");
    setFormCommand("");
    setFormArgs("");
    setTestResult(null);
  };

  const startEdit = (key: string, entry: McpServerEntry) => {
    setEditingKey(key);
    setFormName(key);
    setFormCommand(entry.command);
    setFormArgs((entry.args ?? []).join(" "));
    setTestResult(null);
  };

  const cancelEdit = () => {
    setEditingKey(null);
    setTestResult(null);
  };

  const saveOne = () => {
    const name = formName.trim();
    const command = formCommand.trim();
    if (!name || !command) return;
    const args = formArgs.trim() ? formArgs.trim().split(/\s+/) : [];
    const next: McpConfig = {
      mcpServers: { ...config.mcpServers, [name]: { command, args } },
    };
    if (editingKey && editingKey !== "__new__" && editingKey !== name) {
      delete next.mcpServers[editingKey];
    }
    setConfig(next);
    setEditingKey(null);
  };

  const removeOne = (key: string) => {
    const next = { mcpServers: { ...config.mcpServers } };
    delete next.mcpServers[key];
    setConfig(next);
    if (editingKey === key) setEditingKey(null);
  };

  const handleSaveAll = async () => {
    try {
      setSaving(true);
      await saveMcpConfig(config, true);
      setSaveFeedback(true);
      setTimeout(() => setSaveFeedback(false), 1500);
    } catch (e) {
      window.alert(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async (key: string) => {
    const entry = config.mcpServers[key];
    if (!entry) return;
    setTestResult({ key });
    try {
      const count = await testMcpServer(entry);
      setTestResult({ key, count });
    } catch (e) {
      setTestResult({ key, error: e instanceof Error ? e.message : String(e) });
    }
  };

  const entries = Object.entries(config.mcpServers);

  return (
    <div className="mb-8">
      <div className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)] mb-3 pb-2 border-b border-[var(--border)]">
        MCP Servers
      </div>
      <div className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] overflow-hidden">
        <div className="py-4 px-4 border-b border-[var(--border)]">
          <div className="text-[13px] font-semibold text-[var(--text)] mb-1">Configure external MCP servers</div>
          <p className="text-[11px] text-[var(--text-muted)] font-mono leading-relaxed">
            Add stdio servers by command and arguments. Their tools will appear alongside local tools for the agent. Config is stored in app settings and optionally in mcp.json.
          </p>
        </div>
        {appDataDir && (
          <div className="flex items-center justify-between py-3 px-4 border-b border-[var(--border)] gap-4">
            <div className="text-[11px] text-[var(--text-muted)] font-mono">Config file</div>
            <span className="font-mono text-[11px] text-[var(--text-muted)] truncate max-w-[280px]" title={`${appDataDir}${appDataDir.endsWith("/") ? "" : "/"}mcp.json`}>
              {appDataDir}{appDataDir.endsWith("/") ? "" : "/"}mcp.json
            </span>
          </div>
        )}
        {loading ? (
          <div className="py-4 px-4 text-[12px] text-[var(--text-muted)]">Loading…</div>
        ) : (
          <>
            <div className="flex items-center justify-between py-3 px-4 border-b border-[var(--border)]">
              <span className="text-[12px] font-medium text-[var(--text)]">Servers</span>
              <button
                type="button"
                onClick={startAdd}
                className="py-1.5 px-3 rounded-[7px] border border-[var(--border)] text-[var(--text)] font-sans text-[12px] font-medium cursor-pointer hover:bg-[var(--surface-2)]"
              >
                Add server
              </button>
            </div>
            {editingKey !== null && (
              <div className="p-4 border-b border-[var(--border)] bg-[var(--surface-2)]/50 space-y-3">
                <div className="flex gap-2 items-center">
                  <input
                    type="text"
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                    placeholder="Server name (e.g. filesystem)"
                    className="flex-1 min-w-0 bg-[var(--surface)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px]"
                    disabled={editingKey !== "__new__"}
                  />
                </div>
                <div className="flex gap-2 items-center">
                  <input
                    type="text"
                    value={formCommand}
                    onChange={(e) => setFormCommand(e.target.value)}
                    placeholder="Command (e.g. npx)"
                    className="flex-1 min-w-0 bg-[var(--surface)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px]"
                  />
                </div>
                <div className="flex gap-2 items-center">
                  <input
                    type="text"
                    value={formArgs}
                    onChange={(e) => setFormArgs(e.target.value)}
                    placeholder="Arguments (e.g. -y @modelcontextprotocol/server-filesystem)"
                    className="flex-1 min-w-0 bg-[var(--surface)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px]"
                  />
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={saveOne}
                    className="py-1.5 px-3 rounded-[7px] bg-[var(--accent)] text-black font-sans text-[12px] font-semibold cursor-pointer"
                  >
                    {editingKey === "__new__" ? "Add" : "Update"}
                  </button>
                  <button
                    type="button"
                    onClick={cancelEdit}
                    className="py-1.5 px-3 rounded-[7px] border border-[var(--border)] text-[var(--text)] font-sans text-[12px] cursor-pointer hover:bg-[var(--surface-3)]"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}
            {entries.length === 0 && !editingKey && (
              <div className="py-4 px-4 text-[12px] text-[var(--text-muted)]">No servers configured. Click Add server to add one.</div>
            )}
            {entries.map(([key, entry]) => (
              <div
                key={key}
                className="flex items-center justify-between py-3 px-4 border-b border-[var(--border)] last:border-b-0 hover:bg-[var(--surface-2)]/50 gap-2"
              >
                <div className="min-w-0 flex-1">
                  <div className="font-mono text-[12px] font-medium text-[var(--text)]">{key}</div>
                  <div className="font-mono text-[11px] text-[var(--text-muted)] truncate">
                    {entry.command} {(entry.args ?? []).join(" ")}
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {testResult?.key === key && (
                    <span className="text-[11px] text-[var(--text-muted)]">
                      {testResult.error != null ? testResult.error : `${testResult.count ?? 0} tools`}
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={() => handleTest(key)}
                    className="py-1 px-2 rounded border border-[var(--border)] text-[11px] font-medium text-[var(--text)] hover:bg-[var(--surface-3)]"
                  >
                    Test
                  </button>
                  <button
                    type="button"
                    onClick={() => startEdit(key, entry)}
                    className="py-1 px-2 rounded border border-[var(--border)] text-[11px] font-medium text-[var(--text)] hover:bg-[var(--surface-3)]"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => removeOne(key)}
                    className="py-1 px-2 rounded border border-[var(--red)]/50 text-[11px] font-medium text-[var(--red)] hover:bg-[var(--red)]/10"
                  >
                    Remove
                  </button>
                </div>
              </div>
            ))}
            <div className="flex items-center justify-end gap-2 py-4 px-4 border-t border-[var(--border)]">
              <button
                type="button"
                onClick={handleSaveAll}
                disabled={saving}
                className="py-2 px-[18px] bg-[var(--accent)] border-none rounded-[7px] text-black font-sans text-[13px] font-bold cursor-pointer transition-all hover:bg-[#00ffaa] disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {saving ? "Saving…" : saveFeedback ? "Saved" : "Save and reconnect"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export function Settings() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const sectionParam = searchParams.get("section") as SectionId | null;
  const [activeSection, setActiveSection] = useState<SectionId>(
    sectionParam && SECTIONS.some((s) => s.id === sectionParam) ? sectionParam : "ai"
  );
  const [saveFeedback, setSaveFeedback] = useState(false);
  const [clearHistoryFeedback, setClearHistoryFeedback] = useState(false);

  const load = useSettingsStore((s) => s.load);
  const llmProvider = useSettingsStore((s) => s.llmProvider);
  const llmApiKey = useSettingsStore((s) => s.llmApiKey);
  const llmBaseUrl = useSettingsStore((s) => s.llmBaseUrl);
  const llmModel = useSettingsStore((s) => s.llmModel);
  const executionMode = useSettingsStore((s) => s.executionMode);
  const appDataDir = useSettingsStore((s) => s.appDataDir);
  const tools = useSettingsStore((s) => s.tools);
  const testStatus = useSettingsStore((s) => s.testStatus);
  const testMessage = useSettingsStore((s) => s.testMessage);
  const setLlmProvider = useSettingsStore((s) => s.setLlmProvider);
  const setLlmApiKey = useSettingsStore((s) => s.setLlmApiKey);
  const setLlmBaseUrl = useSettingsStore((s) => s.setLlmBaseUrl);
  const setLlmModel = useSettingsStore((s) => s.setLlmModel);
  const setExecutionMode = useSettingsStore((s) => s.setExecutionMode);
  const toolTimeoutMinutes = useSettingsStore((s) => s.toolTimeoutMinutes);
  const setToolTimeoutMinutes = useSettingsStore((s) => s.setToolTimeoutMinutes);
  const runTestConnection = useSettingsStore((s) => s.runTestConnection);
  const clearTestResult = useSettingsStore((s) => s.clearTestResult);
  const manifest = useModelsConfigStore((s) => s.manifest);

  useEffect(() => {
    load();
  }, [load]);

  // Sync active section when URL has ?section= (e.g. deep link from Home)
  useEffect(() => {
    const s = searchParams.get("section") as SectionId | null;
    if (s && SECTIONS.some((sec) => sec.id === s)) setActiveSection(s);
  }, [searchParams]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        navigate("/");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate]);

  const save = async (key: string, value: string) => {
    await saveSetting(key, value);
  };

  const handleSaveAll = async () => {
    await saveSetting("llm_provider", llmProvider);
    await saveSetting("llm_api_key", llmApiKey);
    await saveSetting("llm_base_url", llmBaseUrl);
    await saveSetting("llm_model", llmModel);
    await saveSetting("execution_mode", executionMode);
    await saveSetting("tool_timeout_minutes", String(toolTimeoutMinutes));
    clearTestResult();
    setSaveFeedback(true);
    setTimeout(() => setSaveFeedback(false), 1500);
  };

  return (
    <div className="flex-1 flex flex-col overflow-hidden z-[1] bg-[var(--bg)]">
      {/* Header */}
      <div className="flex items-center gap-2 py-4 px-7 border-b border-[var(--border)] bg-[var(--surface)] flex-shrink-0">
        <button
          type="button"
          onClick={() => navigate("/")}
          className="p-1.5 rounded-[7px] border border-transparent text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--surface-2)] hover:border-[var(--border)] transition-all cursor-pointer"
          aria-label="Close settings"
        >
          <svg className="w-5 h-5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
        <span className="text-[15px] font-bold text-[var(--text)]">Settings</span>
      </div>

      {/* Body: nav + content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left nav */}
        <nav className="w-[180px] min-w-[180px] border-r border-[var(--border)] py-4 px-2.5 flex flex-col gap-0.5 bg-[var(--surface)]">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() => setActiveSection(s.id)}
              className={`flex items-center gap-2 py-2 px-2.5 rounded-[7px] cursor-pointer transition-all text-left text-[13px] font-medium ${
                activeSection === s.id
                  ? "bg-[var(--surface-2)] text-[var(--accent)] border border-[var(--border)]"
                  : "text-[var(--text-muted)] hover:bg-[var(--surface-2)] hover:text-[var(--text)] border border-transparent"
              }`}
            >
              <s.Icon className={iconClass} />
              {s.label}
            </button>
          ))}
        </nav>

        {/* Content */}
        <div className="flex-1 overflow-y-auto py-6 px-7 pb-10">
          {/* AI Provider */}
          {activeSection === "ai" && (
            <div className="mb-8">
              <div className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)] mb-3 pb-2 border-b border-[var(--border)]">
                AI Provider
              </div>
              <div className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] overflow-hidden">
                {/* Connection status (read-only). Control lives next to credentials below. */}
                <div className="flex items-center gap-4 py-3.5 px-4 border-b border-[var(--border)] bg-[var(--surface-2)]/50">
                  {testStatus === "ok" ? (
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="w-2 h-2 rounded-full bg-[var(--accent)] shrink-0" style={{ boxShadow: "0 0 6px var(--accent)" }} aria-hidden />
                      <span className="text-[13px] font-medium text-[var(--text)] truncate">
                        Connected: {LLM_PROVIDERS.find((p) => p.value === llmProvider)?.label ?? llmProvider} · {llmModel}
                      </span>
                    </div>
                  ) : (
                    <p className="text-[13px] text-[var(--text-muted)]">
                      {testStatus === "testing"
                        ? "Testing connection…"
                        : "No active connection. Configure below and use Test connection to connect."}
                    </p>
                  )}
                </div>
                <div className="flex items-center justify-between py-3.5 px-4 border-b border-[var(--border)] gap-4">
                  <div className="flex-1">
                    <div className="text-[13px] font-semibold text-[var(--text)]">Provider</div>
                    <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">All providers use the same interface internally</div>
                  </div>
                  <div className="flex-shrink-0">
                    <select
                      value={isProviderId(llmProvider) ? llmProvider : "ollama"}
                      onChange={(e) => {
                        const value = e.target.value as "ollama" | "openai" | "claude" | "gemini";
                        if (!isProviderId(value)) return;
                        setLlmProvider(value);
                        save("llm_provider", value);
                        const defaultModelId = manifest.providers[value]?.defaultModelId ?? "llama3.2";
                        setLlmModel(defaultModelId);
                        save("llm_model", defaultModelId);
                        clearTestResult();
                      }}
                      className="w-[160px] bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-sans text-[12px] outline-none transition-[border-color] focus:border-[rgba(0,255,136,0.3)] cursor-pointer"
                    >
                      {LLM_PROVIDERS.map((p) => (
                        <option key={p.value} value={p.value}>
                          {p.label}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>
                <div className="flex items-center justify-between py-3.5 px-4 border-b border-[var(--border)] gap-4">
                  <div className="flex-1">
                    <div className="text-[13px] font-semibold text-[var(--text)]">Model</div>
                    <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">Model used for agent reasoning</div>
                  </div>
                  <div className="flex-shrink-0">
                    {(() => {
                      const providerKey = isProviderId(llmProvider) ? llmProvider : "ollama";
                      const providerConfig = manifest.providers[providerKey];
                      const modelList = providerConfig?.models ?? [];
                      if (modelList.length === 0) {
                        return (
                          <input
                            type="text"
                            value={llmModel}
                            onChange={(e) => {
                              setLlmModel(e.target.value);
                              clearTestResult();
                            }}
                            onBlur={() => save("llm_model", llmModel)}
                            placeholder="e.g. llama3.2"
                            className="w-[280px] max-w-full bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px] outline-none transition-[border-color] focus:border-[rgba(0,255,136,0.3)] placeholder:text-[var(--text-dim)] cursor-text"
                          />
                        );
                      }
                      const hasCurrent = modelList.some((m) => m.id === llmModel);
                      const options = hasCurrent ? modelList : [{ id: llmModel, name: `Custom: ${llmModel}`, tags: [] }, ...modelList];
                      return (
                        <select
                          value={llmModel}
                          onChange={(e) => {
                            setLlmModel(e.target.value);
                            save("llm_model", e.target.value);
                            clearTestResult();
                          }}
                          className="w-[320px] max-w-full bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-sans text-[12px] outline-none transition-[border-color] focus:border-[rgba(0,255,136,0.3)] cursor-pointer"
                        >
                          {options.map((m) => (
                            <option key={m.id} value={m.id}>
                              {m.name}
                            </option>
                          ))}
                        </select>
                      );
                    })()}
                  </div>
                </div>
                {llmProvider !== "ollama" && (
                  <div className="flex items-center justify-between py-3.5 px-4 border-b border-[var(--border)] gap-4">
                    <div className="flex-1">
                      <div className="text-[13px] font-semibold text-[var(--text)]">API Key</div>
                      <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">
                        {llmProvider === "gemini"
                          ? "Stored locally. Get key at aistudio.google.com"
                          : "Stored locally, never sent anywhere else"}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 flex-shrink-0">
                      {testStatus === "ok" && llmApiKey.trim().length > 0 ? (
                        <span
                          className="w-[280px] bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text-muted)] font-mono text-[12px] select-none"
                          title="Key stored; disconnect to edit"
                        >
                          {maskApiKey(llmApiKey)}
                        </span>
                      ) : (
                        <input
                          type="text"
                          value={llmApiKey}
                          onChange={(e) => {
                            setLlmApiKey(e.target.value);
                            clearTestResult();
                          }}
                          onBlur={() => save("llm_api_key", llmApiKey)}
                          placeholder={
                            llmProvider === "gemini"
                              ? "Paste key from Google AI Studio"
                              : llmProvider === "claude"
                                ? "sk-ant-••••••••••••••••"
                                : "sk-••••••••••••••••"
                          }
                          className="w-[280px] bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px] outline-none transition-[border-color] focus:border-[rgba(0,255,136,0.3)] placeholder:text-[var(--text-dim)]"
                          autoComplete="off"
                        />
                      )}
                      {testStatus === "ok" ? (
                        <button
                          type="button"
                          onClick={() => clearTestResult()}
                          className="min-w-[120px] py-2 px-4 rounded-[7px] border-2 border-[var(--red)]/60 bg-[var(--red)]/10 text-[var(--red)] font-sans text-[12px] font-semibold cursor-pointer hover:bg-[var(--red)]/20 hover:border-[var(--red)] transition-colors whitespace-nowrap"
                        >
                          Disconnect
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={runTestConnection}
                          disabled={testStatus === "testing"}
                          className="min-w-[120px] py-2 px-4 rounded-[7px] border-2 border-[var(--border)] text-[var(--text)] font-sans text-[12px] font-semibold cursor-pointer transition-all whitespace-nowrap bg-[var(--surface-2)] hover:border-[var(--border-hover)] hover:bg-[var(--surface-3)] disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {testStatus === "testing" ? "Testing…" : "Test connection"}
                        </button>
                      )}
                    </div>
                  </div>
                )}
                {llmProvider === "ollama" && (
                  <div className="flex items-center justify-between py-3.5 px-4 border-b border-[var(--border)] gap-4">
                    <div className="flex-1">
                      <div className="text-[13px] font-semibold text-[var(--text)]">Ollama URL</div>
                      <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">Local Ollama instance endpoint</div>
                    </div>
                    <div className="flex items-center gap-2 flex-shrink-0">
                      <input
                        type="text"
                        value={llmBaseUrl}
                        onChange={(e) => {
                          setLlmBaseUrl(e.target.value);
                          clearTestResult();
                        }}
                        onBlur={() => save("llm_base_url", llmBaseUrl)}
                        className="w-[280px] bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px] outline-none transition-[border-color] focus:border-[rgba(0,255,136,0.3)] placeholder:text-[var(--text-dim)]"
                      />
                      {testStatus === "ok" ? (
                        <button
                          type="button"
                          onClick={() => clearTestResult()}
                          className="min-w-[120px] py-2 px-4 rounded-[7px] border-2 border-[var(--red)]/60 bg-[var(--red)]/10 text-[var(--red)] font-sans text-[12px] font-semibold cursor-pointer hover:bg-[var(--red)]/20 hover:border-[var(--red)] transition-colors whitespace-nowrap"
                        >
                          Disconnect
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={runTestConnection}
                          disabled={testStatus === "testing"}
                          className="min-w-[120px] py-2 px-4 rounded-[7px] border-2 border-[var(--border)] text-[var(--text)] font-sans text-[12px] font-semibold cursor-pointer transition-all whitespace-nowrap bg-[var(--surface-2)] hover:border-[var(--border-hover)] hover:bg-[var(--surface-3)] disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {testStatus === "testing" ? "Testing…" : "Test connection"}
                        </button>
                      )}
                    </div>
                  </div>
                )}
              </div>
              {testMessage && testStatus !== "ok" && (
                <p className="mt-2 text-[12px] text-[var(--red)]">{testMessage}</p>
              )}
            </div>
          )}

          {/* Governance */}
          {activeSection === "governance" && (
            <div className="mb-8">
              <div className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)] mb-3 pb-2 border-b border-[var(--border)]">
                Execution mode
              </div>
              <div className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] overflow-hidden">
                {(
                  [
                    {
                      id: "manual" as const,
                      label: "Manual",
                      description: "Always require approval before running any tool.",
                    },
                    {
                      id: "guided" as const,
                      label: "Guided",
                      description: "Auto-run passive tools; require approval for active and high-impact tools.",
                    },
                    {
                      id: "autonomous" as const,
                      label: "Autonomous",
                      description: "Run all tools automatically without per-action approval.",
                    },
                  ] as const
                ).map((opt) => (
                  <button
                    key={opt.id}
                    type="button"
                    onClick={() => {
                      setExecutionMode(opt.id);
                      save("execution_mode", opt.id);
                    }}
                    className={`w-full flex items-start gap-3 py-3.5 px-4 border-b border-[var(--border)] last:border-b-0 text-left transition-colors ${
                      executionMode === opt.id
                        ? "bg-[var(--accent-dim)] border-l-4 border-l-[var(--accent)]"
                        : "hover:bg-[var(--surface-2)]"
                    }`}
                  >
                    <span
                      className={`w-4 h-4 rounded-full border-2 flex-shrink-0 mt-0.5 ${
                        executionMode === opt.id ? "border-[var(--accent)] bg-[var(--accent)]" : "border-[var(--border)]"
                      }`}
                    />
                    <div className="flex-1 min-w-0">
                      <div className="text-[13px] font-semibold text-[var(--text)]">{opt.label}</div>
                      <div className="text-[11px] text-[var(--text-muted)] mt-0.5">{opt.description}</div>
                    </div>
                  </button>
                ))}
              </div>
              <div className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)] mb-3 pb-2 border-b border-[var(--border)] mt-8">
                Tool timeout
              </div>
              <div className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] overflow-hidden">
                <div className="flex items-center justify-between py-3.5 px-4 gap-4">
                  <div className="flex-1">
                    <div className="text-[13px] font-semibold text-[var(--text)]">Max run time</div>
                    <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">
                      How long a tool can run before it is automatically stopped. Default is 15 minutes.
                    </div>
                  </div>
                  <div className="flex items-center gap-2 flex-shrink-0">
                    <input
                      type="number"
                      min={1}
                      max={120}
                      value={toolTimeoutMinutes}
                      onChange={(e) => {
                        const v = parseInt(e.target.value, 10);
                        if (!Number.isNaN(v) && v >= 1 && v <= 120) {
                          setToolTimeoutMinutes(v);
                          save("tool_timeout_minutes", String(v));
                        }
                      }}
                      className="w-[72px] bg-[var(--surface-2)] border border-[var(--border)] rounded-[7px] py-2 px-3 text-[var(--text)] font-mono text-[12px] outline-none transition-[border-color] focus:border-[rgba(0,255,136,0.3)] text-center"
                    />
                    <span className="text-[12px] text-[var(--text-muted)]">minutes</span>
                  </div>
                </div>
              </div>

              <div className="mt-4 flex justify-end">
                <button
                  type="button"
                  onClick={handleSaveAll}
                  className="py-2 px-[18px] bg-[var(--accent)] border-none rounded-[7px] text-black font-sans text-[13px] font-bold cursor-pointer transition-all hover:bg-[#00ffaa]"
                >
                  {saveFeedback ? "Saved" : "Save Changes"}
                </button>
              </div>
            </div>
          )}

          {/* Data & Storage */}
          {activeSection === "data" && (
            <div className="mb-8">
              <div className="text-[12px] font-bold tracking-[0.1em] uppercase text-[var(--text-dim)] mb-3 pb-2 border-b border-[var(--border)]">
                Storage
              </div>
              <div className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] overflow-hidden">
                <div className="flex items-center justify-between py-3.5 px-4 gap-4">
                  <div className="flex-1">
                    <div className="text-[13px] font-semibold text-[var(--text)]">Data directory</div>
                    <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">SQLite database, audit log, and attachments</div>
                  </div>
                  <div className="flex items-center gap-2 flex-shrink-0">
                    <span className="font-mono text-[11px] text-[var(--text-muted)] truncate max-w-[320px]">
                      {appDataDir || "—"}
                    </span>
                  </div>
                </div>
                <div className="flex items-center justify-between py-3.5 px-4 gap-4 border-t border-[var(--border)]">
                  <div className="flex-1">
                    <div className="text-[13px] font-semibold text-[var(--text)]">Clear chat history</div>
                    <div className="text-[11px] text-[var(--text-muted)] font-mono mt-0.5">Permanently delete all conversations and messages. This cannot be undone.</div>
                  </div>
                  <div className="flex items-center gap-2 flex-shrink-0">
                    {clearHistoryFeedback && (
                      <span className="text-[12px] text-[var(--accent)] font-medium">Cleared</span>
                    )}
                    <button
                      type="button"
                      onClick={async () => {
                        if (!window.confirm("Clear all chat history? All conversations and messages will be permanently deleted. This cannot be undone.")) return;
                        try {
                          await clearAllChatHistory();
                          setClearHistoryFeedback(true);
                          setTimeout(() => setClearHistoryFeedback(false), 2500);
                          navigate("/");
                        } catch (e) {
                          window.alert(e instanceof Error ? e.message : String(e));
                        }
                      }}
                      className="py-2 px-4 rounded-[7px] border-2 border-[var(--red)]/50 bg-transparent text-[var(--red)] font-sans text-[12px] font-semibold cursor-pointer transition-all hover:bg-[var(--red)]/10 hover:border-[var(--red)]/70"
                    >
                      Clear all
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Local Tools */}
          {activeSection === "local-tools" && (
            <LocalToolsSection tools={tools.filter((t) => t.source === "local")} />
          )}

          {/* MCP Servers */}
          {activeSection === "mcp" && (
            <McpServersSection appDataDir={appDataDir} />
          )}
        </div>
      </div>
    </div>
  );
}
