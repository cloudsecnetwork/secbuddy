import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export async function getAppDataDir(): Promise<string> {
  return invoke("get_app_data_dir");
}

export async function saveSetting(key: string, value: string): Promise<void> {
  return invoke("save_setting", { key, value });
}

export async function getSetting(key: string): Promise<string | null> {
  return invoke("get_setting", { key });
}

export async function deleteSetting(key: string): Promise<void> {
  return invoke("delete_setting", { key });
}

export async function refreshLocalTools(): Promise<void> {
  return invoke("refresh_local_tools");
}

export async function listTools(): Promise<
  Array<{ name: string; available: boolean; source: string; detected_path?: string }>
> {
  return invoke("list_tools");
}

export async function testConnection(): Promise<void> {
  return invoke("test_connection");
}

// ---- MCP servers ----

export type McpServerEntry = {
  command: string;
  args?: string[];
  env?: Record<string, string>;
};

export type McpConfig = {
  mcpServers: Record<string, McpServerEntry>;
};

export async function getMcpConfig(): Promise<McpConfig> {
  const raw = await invoke<{ mcpServers?: Record<string, McpServerEntry> }>(
    "get_mcp_config"
  );
  return {
    mcpServers: raw?.mcpServers ?? {},
  };
}

export async function saveMcpConfig(
  config: McpConfig,
  writeFile: boolean = false
): Promise<void> {
  return invoke("save_mcp_config", {
    config: { mcpServers: config.mcpServers },
    writeFile,
  });
}

export async function reloadMcpServers(): Promise<void> {
  return invoke("reload_mcp_servers");
}

export async function testMcpServer(entry: McpServerEntry): Promise<number> {
  return invoke("test_mcp_server", { entry });
}

export async function sendMessage(
  chatId: string,
  content: string
  // attachmentIds?: string[] — attach files / artifacts deferred to later phase
): Promise<void> {
  return invoke("send_message", {
    chatId,
    content,
    // attachmentIds: attachmentIds ?? null,
  });
}

// Attach files / artifacts in chat deferred to later phase (can be hefty).
// export async function attachFileToChat(
//   chatId: string,
//   filePath: string
// ): Promise<string> {
//   return invoke("attach_file_to_chat", { chatId, filePath });
// }

export async function createChat(title: string, mode?: string): Promise<string> {
  return invoke("create_chat", { title, mode: mode ?? null });
}

export async function listChats(): Promise<
  Array<[string, string, string, number]>
> {
  return invoke("list_chats");
}

export async function getChatInfo(
  chatId: string
): Promise<{ title: string | null; mode: string | null }> {
  const [title, mode] = await invoke<[string | null, string | null]>(
    "get_chat_info",
    { chatId }
  );
  return { title: title ?? null, mode: mode ?? null };
}

export async function getChatHistory(
  chatId: string
): Promise<
  Array<[string, string, string, string, string | null, number]>
> {
  return invoke("get_chat_history", { chatId });
}

export type ToolInvocationRow = [
  string,   // 0 id
  string,   // 1 chat_id
  string,   // 2 tool_name
  string,   // 3 tool_source
  string,   // 4 input_params
  string,   // 5 target
  string | null,   // 6 raw_output
  number | null,   // 7 exit_code
  number | null,   // 8 duration_ms
  string | null,   // 9 approval_id
  string,   // 10 status
  string | null,   // 11 phase_name
  string | null,   // 12 risk_category
  number,   // 13 created_at
];

export async function getToolInvocationsForChat(
  chatId: string
): Promise<ToolInvocationRow[]> {
  return invoke("get_tool_invocations_for_chat", { chatId });
}

export type FindingRow = [
  string,
  string,
  string | null,
  string,
  string,
  string,
  string | null,
  string | null,
  string | null,
  string | null,
  number,
];

export async function getFindingsForChat(
  chatId: string
): Promise<FindingRow[]> {
  return invoke("get_findings_for_chat", { chatId });
}

export async function deleteChat(chatId: string): Promise<void> {
  return invoke("delete_chat", { chatId });
}

export async function clearAllChatHistory(): Promise<void> {
  return invoke("clear_all_chat_history");
}

export async function recordApprovalAndExecute(
  invocationId: string,
  decision: "approved" | "denied" | "dry_run"
): Promise<void> {
  return invoke("record_approval_and_execute", {
    invocationId,
    decision,
  });
}

export async function cancelToolInvocation(invocationId: string): Promise<void> {
  return invoke("cancel_tool_invocation", { invocationId });
}

export type ChatEventPayload =
  | { type: "MessageChunk"; content: string }
  | { type: "MessageComplete"; message_id: string }
  | { type: "ToolRunning"; invocation_id: string; tool_name?: string; args?: string; risk_category?: string; phase_name?: string | null }
  | {
      type: "ToolComplete";
      invocation_id: string;
      output: string;
      duration_ms: number | null;
      /** "complete" | "failed" (e.g. cancelled, timeout, error) */
      status?: string;
      phase_name?: string | null;
    }
  | { type: "ToolDenied"; invocation_id: string; reason?: string }
  | {
      type: "ApprovalRequired";
      invocation_id: string;
      tool_name: string;
      args: string;
      target: string;
      risk_category?: string;
    }
  | {
      type: "ConfidencePreview";
      explanation: string;
      what_will_be_tested?: string;
      tool_count?: number;
      execution_plan?: Array<{
        tool_name: string;
        args: string;
        target: string;
        risk_category: string;
        requires_approval: boolean;
      }>;
    }
  | {
      type: "FindingFound";
      id: string;
      title: string;
      severity: string;
      description: string;
    }
  | { type: "Error"; message: string; message_id: string }
  | { type: "AgentStopped" };

export function subscribeChatEvent(
  handler: (payload: ChatEventPayload) => void
): Promise<() => void> {
  return listen<ChatEventPayload>("chat_event", (event) => {
    handler(event.payload);
  }).then((unlisten) => unlisten);
}
