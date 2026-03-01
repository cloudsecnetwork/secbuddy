import { useEffect, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import {
  getChatHistory,
  getToolInvocationsForChat,
  getFindingsForChat,
  getChatInfo,
  subscribeChatEvent,
} from "../lib/tauri";
import type { ToolInvocationRow, FindingRow } from "../lib/tauri";
import type { Message, ToolInvocation, Finding } from "../stores/chat";
import { useChatStore } from "../stores/chat";
import { MessageList } from "../components/MessageList";
import { ChatComposer } from "../components/ChatComposer";

const SCROLL_NEAR_BOTTOM_THRESHOLD_PX = 120;

function parseMessage(row: [string, string, string, string, string | null, number]): Message {
  return {
    id: row[0],
    role: row[2],
    content: row[3],
    toolInvocationId: row[4],
    createdAt: row[5],
  };
}

function parseToolInvocation(row: ToolInvocationRow): ToolInvocation {
  return {
    id: row[0],
    toolName: row[2],
    toolSource: row[3],
    inputParams: row[4],
    target: row[5] || null,
    rawOutput: row[6],
    exitCode: row[7],
    durationMs: row[8],
    blastRadiusScore: 0,
    status: row[10],
    phaseName: row[11],
    riskCategory: row[12] ?? null,
    createdAt: row[13],
  };
}

function parseFinding(row: FindingRow): Finding {
  return {
    id: row[0],
    toolInvocationId: row[2],
    title: row[3],
    severity: row[4],
    description: row[5],
    mitreRef: row[6],
    owaspRef: row[7],
    cweRef: row[8],
    recommendedAction: row[9],
    createdAt: row[10],
  };
}

function isNearBottom(el: HTMLDivElement, threshold = SCROLL_NEAR_BOTTOM_THRESHOLD_PX): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= threshold;
}

export function Chat() {
  const { chatId } = useParams<{ chatId: string }>();
  const [chatTitle, setChatTitle] = useState<string>("");
  const [chatMode, setChatMode] = useState<string>("");
  const [showJumpToLatest, setShowJumpToLatest] = useState(false);
  const resetForChat = useChatStore((s) => s.resetForChat);
  const setMessages = useChatStore((s) => s.setMessages);
  const setToolInvocations = useChatStore((s) => s.setToolInvocations);
  const setFindings = useChatStore((s) => s.setFindings);
  const applyChatEvent = useChatStore((s) => s.applyChatEvent);
  const scrollRef = useRef<HTMLDivElement>(null);
  const nearBottomRef = useRef(true);
  const openScrollToBottomRef = useRef(false);

  useEffect(() => {
    if (!chatId) return;
    resetForChat(chatId);
    openScrollToBottomRef.current = true;
    getChatInfo(chatId)
      .then(({ title, mode }) => {
        setChatTitle(title ?? "Chat");
        setChatMode(mode ?? "auto");
      })
      .catch((e) => console.error("getChatInfo failed", e));
    Promise.all([
      getChatHistory(chatId),
      getToolInvocationsForChat(chatId),
      getFindingsForChat(chatId),
    ])
      .then(([msgRows, invRows, findRows]) => {
        setMessages(msgRows.map(parseMessage));
        setToolInvocations(invRows.map(parseToolInvocation));
        setFindings(findRows.map(parseFinding));
      })
      .catch((e) => console.error("Failed to load chat history", e));
  }, [chatId, resetForChat, setMessages, setToolInvocations, setFindings]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    subscribeChatEvent((payload) => {
      if (!cancelled) applyChatEvent(payload);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    }).catch((e) => console.error("subscribeChatEvent failed", e));

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [applyChatEvent]);

  const messages = useChatStore((s) => s.messages);
  const toolInvocations = useChatStore((s) => s.toolInvocations);
  const streamingContent = useChatStore((s) => s.streamingContent);

  const scrollToBottom = () => {
    const el = scrollRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
      setShowJumpToLatest(false);
    }
  };

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    nearBottomRef.current = isNearBottom(el);
    setShowJumpToLatest((prev) => (nearBottomRef.current ? false : true));
  };

  // When user sends a message: scroll to latest immediately.
  // When reopening a chat: scroll to bottom after messages load.
  // While streaming: only auto-scroll if user is at/near bottom.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    const lastMsg = messages[messages.length - 1];
    const userJustSent = lastMsg?.role === "user";
    const hasContent = messages.length > 0 || Object.keys(toolInvocations).length > 0;

    if (openScrollToBottomRef.current && hasContent) {
      openScrollToBottomRef.current = false;
      scrollToBottom();
      return;
    }
    if (userJustSent) {
      scrollToBottom();
      return;
    }
    if (nearBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages.length, messages, toolInvocations, streamingContent]);

  if (!chatId) return null;

  return (
    <div className="flex flex-col h-full min-h-0 bg-[var(--bg)] relative z-[1]">
      <header className="flex items-center justify-between py-3.5 px-5 border-b border-[var(--border)] bg-[var(--surface)] flex-shrink-0">
        <div className="flex items-center gap-3">
          <span className="text-[14px] font-semibold text-[var(--text)] truncate max-w-[320px]">
            {chatTitle || "Chat"}
          </span>
          <span className="font-mono text-[10px] font-medium py-1 px-2 rounded border bg-[var(--accent-dim)] text-[var(--accent)] border-[rgba(0,255,136,0.2)] tracking-wide uppercase shrink-0">
            {chatMode === "auto" ? "Auto" : chatMode.charAt(0).toUpperCase() + chatMode.slice(1)}
          </span>
        </div>
      </header>
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden relative">
        <MessageList scrollContainerRef={scrollRef} onScroll={handleScroll} />
        {showJumpToLatest && (
          <div className="absolute bottom-4 left-1/2 -translate-x-1/2 z-10">
            <button
              type="button"
              onClick={scrollToBottom}
              className="py-2 px-4 rounded-lg border border-[var(--border)] bg-[var(--surface)] text-[var(--text)] font-sans text-[13px] font-medium shadow-lg hover:bg-[var(--surface-2)] hover:border-[var(--border-hover)] transition-colors"
            >
              New messages ↓
            </button>
          </div>
        )}
      </div>
      <ChatComposer chatId={chatId} />
    </div>
  );
}
