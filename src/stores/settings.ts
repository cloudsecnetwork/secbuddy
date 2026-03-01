import { create } from "zustand";
import {
  getSetting,
  getAppDataDir,
  refreshLocalTools,
  listTools,
  testConnection,
} from "../lib/tauri";

export type ToolInfo = {
  name: string;
  available: boolean;
  source: string;
  detected_path?: string;
};

export type ExecutionMode = "manual" | "guided" | "autonomous";

type SettingsState = {
  llmProvider: string;
  llmApiKey: string;
  llmBaseUrl: string;
  llmModel: string;
  executionMode: ExecutionMode;
  toolTimeoutMinutes: number;
  appDataDir: string;
  tools: ToolInfo[];
  testStatus: "idle" | "testing" | "ok" | "error";
  testMessage: string;
  load: () => Promise<void>;
  setLlmProvider: (v: string) => void;
  setLlmApiKey: (v: string) => void;
  setLlmBaseUrl: (v: string) => void;
  setLlmModel: (v: string) => void;
  setExecutionMode: (v: ExecutionMode) => void;
  setToolTimeoutMinutes: (v: number) => void;
  setAppDataDir: (v: string) => void;
  setTools: (t: ToolInfo[]) => void;
  runTestConnection: () => Promise<void>;
  clearTestResult: () => void;
};

function executionModeFromThreshold(threshold: string | null): ExecutionMode {
  const n = parseInt(threshold ?? "", 10);
  if (Number.isNaN(n)) return "guided";
  if (n <= 0) return "autonomous";
  if (n >= 100) return "manual";
  return "guided";
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  llmProvider: "ollama",
  llmApiKey: "",
  llmBaseUrl: "http://localhost:11434",
  llmModel: "llama3.2",
  executionMode: "guided",
  toolTimeoutMinutes: 15,
  appDataDir: "",
  tools: [],
  testStatus: "testing",
  testMessage: "",

  load: async () => {
    const [provider, apiKey, baseUrl, model, executionModeVal, approvalThreshold, toolTimeout, appDir] =
      await Promise.all([
        getSetting("llm_provider"),
        getSetting("llm_api_key"),
        getSetting("llm_base_url"),
        getSetting("llm_model"),
        getSetting("execution_mode"),
        getSetting("approval_threshold"),
        getSetting("tool_timeout_minutes"),
        getAppDataDir(),
      ]);
    let mode: ExecutionMode = "guided";
    if (executionModeVal === "manual" || executionModeVal === "guided" || executionModeVal === "autonomous") {
      mode = executionModeVal;
    } else if (approvalThreshold !== null && approvalThreshold !== undefined) {
      mode = executionModeFromThreshold(approvalThreshold);
      const { saveSetting, deleteSetting } = await import("../lib/tauri");
      await saveSetting("execution_mode", mode);
      await deleteSetting("approval_threshold");
    }
    const parsedTimeout = parseInt(toolTimeout ?? "", 10);
    set({
      llmProvider: provider ?? "ollama",
      llmApiKey: apiKey ?? "",
      llmBaseUrl: baseUrl ?? "http://localhost:11434",
      llmModel: model ?? "llama3.2",
      executionMode: mode,
      toolTimeoutMinutes: Number.isNaN(parsedTimeout) || parsedTimeout < 1 ? 15 : parsedTimeout,
      appDataDir: appDir ?? "",
    });
    await refreshLocalTools();
    const tools = await listTools();
    set({ tools });
  },

  setLlmProvider: (v) => set({ llmProvider: v }),
  setLlmApiKey: (v) => set({ llmApiKey: v }),
  setLlmBaseUrl: (v) => set({ llmBaseUrl: v }),
  setLlmModel: (v) => set({ llmModel: v }),
  setExecutionMode: (v) => set({ executionMode: v }),
  setToolTimeoutMinutes: (v) => set({ toolTimeoutMinutes: v }),
  setAppDataDir: (v) => set({ appDataDir: v }),
  setTools: (t) => set({ tools: t }),

  runTestConnection: async () => {
    set({ testStatus: "testing", testMessage: "" });
    const { llmProvider, llmApiKey, llmBaseUrl, llmModel } = get();
    const { saveSetting } = await import("../lib/tauri");
    await saveSetting("llm_provider", llmProvider);
    await saveSetting("llm_api_key", llmApiKey);
    await saveSetting("llm_base_url", llmBaseUrl);
    try {
      await testConnection();
      await saveSetting("llm_model", llmModel);
      set({ testStatus: "ok", testMessage: "Connected. Settings saved." });
    } catch (e) {
      set({
        testStatus: "error",
        testMessage: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearTestResult: () => set({ testStatus: "idle", testMessage: "" }),
}));
