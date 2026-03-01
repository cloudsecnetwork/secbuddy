import { create } from "zustand";
import type { ChatEventPayload } from "../lib/tauri";

export type Message = {
  id: string;
  role: string;
  content: string;
  toolInvocationId: string | null;
  createdAt: number;
};

export type ToolInvocation = {
  id: string;
  toolName: string;
  toolSource: string;
  inputParams: string;
  target: string | null;
  rawOutput: string | null;
  exitCode: number | null;
  durationMs: number | null;
  blastRadiusScore: number;
  status: string;
  phaseName: string | null;
  riskCategory: string | null;
  createdAt: number;
  /** Set when status becomes "running" (for elapsed-time counter). */
  runningStartedAt?: number | null;
};

export type Finding = {
  id: string;
  toolInvocationId: string | null;
  title: string;
  severity: string;
  description: string;
  mitreRef: string | null;
  owaspRef: string | null;
  cweRef: string | null;
  recommendedAction: string | null;
  createdAt: number;
};

type ChatState = {
  currentChatId: string | null;
  messages: Message[];
  toolInvocations: Record<string, ToolInvocation>;
  findings: Finding[];
  streamingContent: string;
  isWaitingForResponse: boolean;
  setCurrentChat: (id: string | null) => void;
  setMessages: (messages: Message[]) => void;
  setToolInvocations: (invocations: ToolInvocation[]) => void;
  setFindings: (findings: Finding[]) => void;
  setWaitingForResponse: (v: boolean) => void;
  applyChatEvent: (payload: ChatEventPayload) => void;
  clearStreaming: () => void;
  resetForChat: (chatId: string) => void;
};

export const useChatStore = create<ChatState>((set, _get) => ({
  currentChatId: null,
  messages: [],
  toolInvocations: {},
  findings: [],
  streamingContent: "",
  isWaitingForResponse: false,

  setCurrentChat: (id) => set({ currentChatId: id }),
  setWaitingForResponse: (v) => set({ isWaitingForResponse: v }),

  setMessages: (messages) => set({ messages }),

  setToolInvocations: (invocations) => {
    const map: Record<string, ToolInvocation> = {};
    invocations.forEach((t) => {
      map[t.id] = t;
    });
    set({ toolInvocations: map });
  },

  setFindings: (findings) => set({ findings }),

  applyChatEvent: (payload) => {
    switch (payload.type) {
      case "MessageChunk":
        set((s) => ({
          isWaitingForResponse: false,
          streamingContent: s.streamingContent + payload.content,
        }));
        break;
      case "MessageComplete":
        set((s) => {
          if (s.messages.some((m) => m.id === payload.message_id)) {
            return { streamingContent: "", isWaitingForResponse: false };
          }
          const content = s.streamingContent;
          return {
            messages: [
              ...s.messages,
              {
                id: payload.message_id,
                role: "assistant",
                content,
                toolInvocationId: null,
                createdAt: Date.now(),
              },
            ],
            streamingContent: "",
            isWaitingForResponse: false,
          };
        });
        break;
      case "ToolRunning": {
        set((s) => {
          const existing = s.toolInvocations[payload.invocation_id];
          return {
            isWaitingForResponse: false,
            toolInvocations: {
              ...s.toolInvocations,
              [payload.invocation_id]: {
                ...existing,
                id: payload.invocation_id,
                toolName: payload.tool_name ?? existing?.toolName ?? "",
                toolSource: existing?.toolSource ?? "local",
                inputParams: payload.args ?? existing?.inputParams ?? "",
                rawOutput: existing?.rawOutput ?? null,
                exitCode: existing?.exitCode ?? null,
                durationMs: existing?.durationMs ?? null,
                blastRadiusScore: existing?.blastRadiusScore ?? 0,
                status: "running",
                phaseName: payload.phase_name ?? existing?.phaseName ?? null,
                riskCategory: existing?.riskCategory ?? null,
                createdAt: existing?.createdAt ?? Date.now(),
                runningStartedAt: Date.now(),
              },
            },
          };
        });
        break;
      }
      case "ToolComplete":
        set((s) => {
          const existing = s.toolInvocations[payload.invocation_id];
          return {
            toolInvocations: {
              ...s.toolInvocations,
              [payload.invocation_id]: {
                ...existing,
                id: payload.invocation_id,
                rawOutput: payload.output,
                durationMs: payload.duration_ms ?? null,
                status: payload.status ?? "complete",
                phaseName: payload.phase_name ?? existing?.phaseName ?? null,
              },
            },
          };
        });
        break;
      case "ToolDenied":
        set((s) => {
          const existing = s.toolInvocations[payload.invocation_id];
          return {
            toolInvocations: {
              ...s.toolInvocations,
              [payload.invocation_id]: {
                ...existing,
                id: payload.invocation_id,
                status: "denied",
                rawOutput: payload.reason ?? "Denied",
              },
            },
          };
        });
        break;
      case "ApprovalRequired":
        set((s) => ({
          toolInvocations: {
            ...s.toolInvocations,
            [payload.invocation_id]: {
              id: payload.invocation_id,
              toolName: payload.tool_name,
              toolSource: "local",
              inputParams: payload.args,
              target: payload.target ?? null,
              rawOutput: null,
              exitCode: null,
              durationMs: null,
              blastRadiusScore: 0,
              status: "pending",
              phaseName: null,
              riskCategory: payload.risk_category ?? null,
              createdAt: Date.now(),
            },
          },
        }));
        break;
      case "ConfidencePreview":
        break;
      case "FindingFound":
        set((s) => ({
          findings: [
            ...s.findings,
            {
              id: payload.id,
              toolInvocationId: null,
              title: payload.title,
              severity: payload.severity,
              description: payload.description,
              mitreRef: null,
              owaspRef: null,
              cweRef: null,
              recommendedAction: null,
              createdAt: Date.now(),
            },
          ],
        }));
        break;
      case "AgentStopped":
        set({ streamingContent: "", isWaitingForResponse: false });
        break;
      case "Error":
        set((s) => {
          if (s.messages.some((m) => m.id === payload.message_id)) {
            return { streamingContent: "", isWaitingForResponse: false };
          }
          return {
            streamingContent: "",
            isWaitingForResponse: false,
            messages: [
              ...s.messages,
              {
                id: payload.message_id,
                role: "assistant",
                content: `[error] ${payload.message}`,
                toolInvocationId: null,
                createdAt: Date.now(),
              },
            ],
          };
        });
        break;
    }
  },

  clearStreaming: () => set({ streamingContent: "" }),

  resetForChat: (chatId) =>
    set({
      currentChatId: chatId,
      messages: [],
      toolInvocations: {},
      findings: [],
      streamingContent: "",
      isWaitingForResponse: false,
    }),
}));
