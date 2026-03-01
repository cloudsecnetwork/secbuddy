import { useState, useRef, useEffect } from "react";
import { sendMessage } from "../lib/tauri";
import { useChatStore } from "../stores/chat";
import type { Message } from "../stores/chat";

type Props = { chatId: string };

export function ChatComposer({ chatId }: Props) {
  const [input, setInput] = useState("");
  // Attach files / artifacts in chat deferred to later phase (can be hefty).
  // const [attachmentIds, setAttachmentIds] = useState<string[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const addMessage = (msg: Message) =>
    useChatStore.setState((s) => ({ messages: [...s.messages, msg] }));
  const setWaitingForResponse = useChatStore((s) => s.setWaitingForResponse);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
  }, [input]);

  // const handleAttachFile = async () => {
  //   try {
  //     const { open } = await import("@tauri-apps/plugin-dialog");
  //     const selected = await open({ multiple: true, title: "Attach file" });
  //     if (selected === null) return;
  //     const paths = Array.isArray(selected) ? selected : [selected];
  //     const ids: string[] = [];
  //     for (const path of paths) {
  //       try {
  //         const id = await attachFileToChat(chatId, path);
  //         ids.push(id);
  //       } catch (_) {
  //         // skip failed
  //       }
  //     }
  //     setAttachmentIds((prev) => [...prev, ...ids]);
  //   } catch (err) {
  //     console.error(err);
  //   }
  // };

  const handleSubmit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    const text = input.trim();
    if (!text) return;
    setInput("");
    addMessage({
      id: `local-${Date.now()}`,
      role: "user",
      content: text,
      toolInvocationId: null,
      createdAt: Date.now(),
    });
    setWaitingForResponse(true);
    try {
      await sendMessage(chatId, text);
    } catch (err) {
      setWaitingForResponse(false);
      console.error(err);
    }
  };

  return (
    <div className="border-t border-[var(--border)] bg-[var(--surface)] py-3.5 px-5 pb-4 flex-shrink-0">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          handleSubmit(e);
        }}
      >
        <div className="bg-[var(--surface-2)] border border-[var(--border)] rounded-[12px] py-3 px-3.5 transition-[border-color,box-shadow] focus-within:border-[rgba(0,255,136,0.25)] focus-within:shadow-[0_0_0_3px_rgba(0,255,136,0.05)]">
          <div className="flex items-end gap-2.5">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSubmit(e);
                }
              }}
              placeholder="Ask a follow-up or describe next steps..."
              rows={1}
              className="flex-1 bg-transparent border-none outline-none text-[var(--text)] font-sans text-[14px] resize-none min-h-[40px] max-h-[120px] leading-normal placeholder:text-[var(--text-dim)]"
            />
            <button
              type="button"
              className="w-[34px] h-[34px] bg-[var(--accent)] border-none rounded-lg cursor-pointer flex items-center justify-center shrink-0 transition-all hover:bg-[#00ffaa] hover:scale-105 disabled:opacity-50 disabled:cursor-not-allowed"
              onClick={() => handleSubmit()}
              disabled={!input.trim()}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#000" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="pointer-events-none">
                <line x1="22" y1="2" x2="11" y2="13" />
                <polygon points="22 2 15 22 11 13 2 9 22 2" />
              </svg>
            </button>
          </div>
        </div>
      </form>
      {/* {attachmentIds.length > 0 && (
        <p className="mt-1.5 font-mono text-[11px] text-[var(--accent)]">
          {attachmentIds.length} file(s) attached
        </p>
      )} */}
    </div>
  );
}
