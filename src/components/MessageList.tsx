import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useChatStore } from "../stores/chat";
import type { Message, ToolInvocation } from "../stores/chat";
import { ToolCard } from "./ToolCard";
import { FindingCard } from "./FindingCard";

/** Remove stray list markers (e.g. a lone "*") that some LLMs emit before bullet lists. */
function normalizeMarkdownContent(text: string): string {
  return text
    .split("\n")
    .filter((line) => !/^\s*\*\s*$/.test(line))
    .join("\n");
}

const markdownComponents = {
  p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
  ul: ({ children }) => <ul className="list-disc list-inside mb-2 space-y-1">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal list-inside mb-2 space-y-1">{children}</ol>,
  li: ({ children }) => <li className="leading-relaxed">{children}</li>,
  strong: ({ children }) => <strong className="font-semibold text-[var(--text)]">{children}</strong>,
  code: ({ className, children, ...props }) => {
    const isBlock = className?.includes("language-");
    if (isBlock) {
      return (
        <pre className="my-2 p-3 rounded-lg bg-[var(--surface-2)] border border-[var(--border)] overflow-x-auto text-[13px]">
          <code {...props}>{children}</code>
        </pre>
      );
    }
    return (
      <code
        className="px-1.5 py-0.5 rounded bg-[var(--surface-2)] border border-[var(--border)] font-mono text-[13px]"
        {...props}
      >
        {children}
      </code>
    );
  },
};

type TimelineItem =
  | { kind: "message"; msg: Message; ts: number }
  | { kind: "tool"; inv: ToolInvocation; ts: number };

type MessageListProps = {
  scrollContainerRef?: React.RefObject<HTMLDivElement | null>;
  onScroll?: (e: React.UIEvent<HTMLDivElement>) => void;
};

export function MessageList({ scrollContainerRef, onScroll }: MessageListProps = {}) {
  const messages = useChatStore((s) => s.messages);
  const toolInvocations = useChatStore((s) => s.toolInvocations);
  const findings = useChatStore((s) => s.findings);
  const streamingContent = useChatStore((s) => s.streamingContent);
  const isWaitingForResponse = useChatStore((s) => s.isWaitingForResponse);

  const linkedInvIds = new Set(
    messages.filter((m) => m.toolInvocationId).map((m) => m.toolInvocationId),
  );

  const timeline: TimelineItem[] = [
    ...messages.map((msg) => ({ kind: "message" as const, msg, ts: msg.createdAt })),
    ...Object.values(toolInvocations)
      .filter((inv) => !linkedInvIds.has(inv.id))
      .map((inv) => ({ kind: "tool" as const, inv, ts: inv.createdAt })),
  ].sort((a, b) => a.ts - b.ts);

  return (
    <div
      ref={scrollContainerRef}
      onScroll={onScroll}
      className="flex-1 overflow-y-auto py-6 flex flex-col gap-1 px-0 min-h-0"
    >
      {timeline.map((item) => {
        if (item.kind === "tool") {
          const inv = item.inv;
          return (
            <div key={`tool-${inv.id}`} className="px-6 py-2 flex gap-3 items-start">
              <div className="w-7 h-7 shrink-0 mt-0.5 flex-shrink-0" />
              <div className="flex-1 max-w-[680px] min-w-0">
                <ToolCard inv={inv} />
                {findings
                  .filter((f) => f.toolInvocationId === inv.id)
                  .map((f) => (
                    <FindingCard key={f.id} finding={f} />
                  ))}
              </div>
            </div>
          );
        }

        const msg = item.msg;
        const isError = msg.content.startsWith("[error] ");

        return (
          <div key={msg.id} className="px-6 py-2 max-w-full">
            {msg.role === "user" ? (
              <div className="flex justify-end">
                <div className="bg-[var(--surface-2)] border border-[var(--border)] rounded-[12px_12px_3px_12px] py-3 px-4 max-w-[560px] text-[14px] leading-relaxed text-[var(--text)]">
                  {msg.content}
                </div>
              </div>
            ) : isError ? (
              <div className="flex gap-3 items-start">
                <div className="w-7 h-7 rounded-[7px] bg-[var(--red)] flex items-center justify-center shrink-0 mt-0.5 flex-shrink-0">
                  <svg className="w-3.5 h-3.5 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="8" x2="12" y2="12" />
                    <line x1="12" y1="16" x2="12.01" y2="16" />
                  </svg>
                </div>
                <div className="flex-1 max-w-[680px] min-w-0">
                  <div className="rounded-lg border border-[var(--red)]/30 bg-[var(--red-dim)] py-3 px-4">
                    <div className="text-[14px] leading-[1.7] text-[var(--text)] whitespace-pre-wrap">
                      {msg.content.replace(/^\[error\] /, "")}
                    </div>
                  </div>
                </div>
              </div>
            ) : (
              <div className="flex gap-3 items-start">
                <div className="w-7 h-7 rounded-[7px] bg-[var(--accent)] flex items-center justify-center text-[13px] shrink-0 mt-0.5 flex-shrink-0">
                  <svg className="w-3.5 h-3.5 text-black" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                  </svg>
                </div>
                <div className="flex-1 max-w-[680px] min-w-0">
                  <div className="text-[14px] leading-[1.7] text-[var(--text)] mb-3 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
                    <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
                      {normalizeMarkdownContent(msg.content)}
                    </ReactMarkdown>
                  </div>
                  {msg.toolInvocationId && toolInvocations[msg.toolInvocationId] && (
                    <ToolCard inv={toolInvocations[msg.toolInvocationId]} />
                  )}
                  {findings
                    .filter((f) => f.toolInvocationId === msg.toolInvocationId)
                    .map((f) => (
                      <FindingCard key={f.id} finding={f} />
                    ))}
                </div>
              </div>
            )}
          </div>
        );
      })}
      {isWaitingForResponse && !streamingContent && (
        <div className="px-6 py-2 flex gap-3 items-start" role="status" aria-live="polite">
          <div className="w-7 h-7 rounded-[7px] bg-[var(--accent)] flex items-center justify-center shrink-0 mt-0.5 flex-shrink-0">
            <svg className="w-3.5 h-3.5 text-black" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
          </div>
          <div className="flex-1 max-w-[680px] flex items-center min-h-[28px]">
            <span className="inline-flex gap-1">
              <span className="w-2 h-2 rounded-full bg-[var(--text-muted)] animate-[typing-bounce_1.4s_ease-in-out_infinite]" style={{ animationDelay: "0ms" }} />
              <span className="w-2 h-2 rounded-full bg-[var(--text-muted)] animate-[typing-bounce_1.4s_ease-in-out_infinite]" style={{ animationDelay: "160ms" }} />
              <span className="w-2 h-2 rounded-full bg-[var(--text-muted)] animate-[typing-bounce_1.4s_ease-in-out_infinite]" style={{ animationDelay: "320ms" }} />
            </span>
          </div>
        </div>
      )}
      {streamingContent && (
        <div className="px-6 py-2 flex gap-3 items-start">
          <div className="w-7 h-7 rounded-[7px] bg-[var(--accent)] flex items-center justify-center shrink-0 mt-0.5 flex-shrink-0">
            <svg className="w-3.5 h-3.5 text-black" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
          </div>
          <div className="flex-1 max-w-[680px] text-[14px] leading-[1.7] text-[var(--text)] [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
              {normalizeMarkdownContent(streamingContent)}
            </ReactMarkdown>
          </div>
        </div>
      )}
    </div>
  );
}
