import { useState, useRef, useEffect } from "react";
import { useNavigate, Link } from "react-router-dom";
import { createChat, sendMessage } from "../lib/tauri";
import { useChatStore } from "../stores/chat";
import { useSettingsStore } from "../stores/settings";

// Attach files deferred to later phase.
// function fileName(path: string): string {
//   const parts = path.replace(/\/$/, "").split(/[/\\]/);
//   return parts[parts.length - 1] ?? path;
// }

const SUGGESTIONS = [
  { label: "Recon", desc: "Host & domain intelligence", template: "Run recon on TARGET and summarize findings." },
  { label: "Triage", desc: "IOC analysis & correlation", template: "Triage this security alert: TARGET" },
  { label: "Validation", desc: "Verify patches & controls", template: "Validate that the fix for TARGET is effective." },
  { label: "Assessment", desc: "Vulnerabilities & misconfigs", template: "Scan TARGET web application for common vulnerabilities." },
];

const MODES = ["Auto", "Recon", "Triage", "Validation", "Assessment"] as const;

export function Home() {
  const navigate = useNavigate();
  const [input, setInput] = useState("");
  const [mode, setMode] = useState<typeof MODES[number]>("Auto");
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  // const [pendingFiles, setPendingFiles] = useState<Array<{ path: string; name: string }>>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const resetForChat = useChatStore((s) => s.resetForChat);
  const setCurrentChat = useChatStore((s) => s.setCurrentChat);
  const setWaitingForResponse = useChatStore((s) => s.setWaitingForResponse);
  const loadSettings = useSettingsStore((s) => s.load);
  const testStatus = useSettingsStore((s) => s.testStatus);
  const runTestConnection = useSettingsStore((s) => s.runTestConnection);

  // Load settings and run a one-time connection check when status is idle (e.g. first install)
  useEffect(() => {
    let cancelled = false;
    (async () => {
      await loadSettings();
      if (cancelled) return;
      const status = useSettingsStore.getState().testStatus;
      if (status === "idle" || status === "testing") {
        await runTestConnection();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loadSettings, runTestConnection]);

  // const handleAttachFile = async () => {
  //   try {
  //     const { open } = await import("@tauri-apps/plugin-dialog");
  //     const selected = await open({ multiple: true, title: "Attach file" });
  //     if (selected === null) return;
  //     const paths = Array.isArray(selected) ? selected : [selected];
  //     setPendingFiles((prev) => [
  //       ...prev,
  //       ...paths.map((path) => ({ path, name: fileName(path) })),
  //     ]);
  //   } catch (err) {
  //     console.error(err);
  //   }
  // };

  const agentReady = testStatus === "ok";

  const handleSubmit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    setSubmitError(null);
    if (!agentReady) return;
    const text = input.trim();
    if (!text) return;
    if (isSubmitting) return;
    setIsSubmitting(true);
    try {
      const modeValue = mode === "Auto" ? "auto" : mode.toLowerCase();
      const chatId = await createChat(text.slice(0, 80), modeValue);
      resetForChat(chatId);
      setCurrentChat(chatId);
      navigate(`/chat/${chatId}`);
      setWaitingForResponse(true);
      // const attachmentIds: string[] = [];
      // for (const { path } of pendingFiles) {
      //   try {
      //     const id = await attachFileToChat(chatId, path);
      //     attachmentIds.push(id);
      //   } catch (_) {
      //     // skip failed attach
      //   }
      // }
      // setPendingFiles([]);
      await sendMessage(chatId, text);
      // await sendMessage(chatId, text, attachmentIds.length > 0 ? attachmentIds : undefined);
    } catch (err) {
      setWaitingForResponse(false);
      const message = err instanceof Error ? err.message : String(err);
      setSubmitError(message);
      console.error(err);
    } finally {
      setIsSubmitting(false);
    }
  };

  const fillSuggestion = (template: string) => {
    if (!agentReady) return;
    const value = template.replace("TARGET", "example.com");
    setInput(value);
    setTimeout(() => {
      textareaRef.current?.focus();
      textareaRef.current?.setSelectionRange(value.length, value.length);
    }, 0);
  };

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [input]);

  return (
    <div className="flex-1 flex flex-col items-center justify-center relative z-[1] py-10 px-6 overflow-auto">
      {/* Glow orb */}
      <div
        className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-[60%] w-[500px] h-[500px] pointer-events-none"
        style={{
          background: "radial-gradient(circle, rgba(0,255,136,0.04) 0%, transparent 70%)",
        }}
      />

      {/* Hero */}
      <div className="text-center mb-10 animate-[fadeUp_0.5s_ease_both]">
        <div className="font-mono text-[11px] text-[var(--accent)] tracking-[0.15em] uppercase mb-3.5 opacity-80">
          Governed · Secure · Local
        </div>
        <h1 className="text-[42px] font-extrabold tracking-tight leading-tight text-[var(--text)] mb-2.5">
          <span className="text-[var(--accent)]">Security AI Agent</span>
        </h1>
        <p className="text-sm text-[var(--text-muted)] font-normal tracking-wide">
          {agentReady
            ? "Describe a target, paste a finding, or drop a file to get started"
            : "Connect an AI provider in Settings, then describe a target or paste a finding to begin."}
        </p>
      </div>

      {/* Input: disabled when agent not ready so we don't invite a futile action */}
      <div className="w-full max-w-[680px] animate-[fadeUp_0.5s_0.1s_ease_both]">
        <form onSubmit={handleSubmit}>
          <div
            className={`bg-[var(--surface)] border rounded-[14px] p-4 transition-all ${
              agentReady
                ? "border-[var(--border)] focus-within:border-[rgba(0,255,136,0.3)] focus-within:shadow-[0_0_0_3px_rgba(0,255,136,0.06)]"
                : "border-[var(--border)] opacity-60"
            }`}
          >
            <div className="flex items-start gap-3">
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => {
                setInput(e.target.value);
                if (submitError) setSubmitError(null);
              }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    handleSubmit();
                  }
                }}
                placeholder={
                  testStatus === "testing"
                    ? "Connecting to AI provider…"
                    : agentReady
                      ? "e.g. Run recon on api.acme.com and check for exposed services..."
                      : "Connect an AI provider to enable the agent"
                }
                rows={2}
                disabled={!agentReady}
                aria-describedby={!agentReady ? "setup-required-msg" : undefined}
                className="flex-1 bg-transparent border-none outline-none text-[var(--text)] font-sans text-[15px] resize-none min-h-[52px] max-h-[160px] leading-normal placeholder:text-[var(--text-dim)] disabled:cursor-not-allowed disabled:opacity-100"
              />
              <button
                type="submit"
                disabled={!agentReady || isSubmitting}
                aria-label={isSubmitting ? "Sending…" : "Send"}
                className="w-[38px] h-[38px] bg-[var(--accent)] border-none rounded-lg flex items-center justify-center shrink-0 mt-0.5 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:scale-100 enabled:cursor-pointer enabled:hover:bg-[#00ffaa] enabled:hover:scale-105"
              >
                {isSubmitting ? (
                  <span className="w-4 h-4 border-2 border-[#000] border-t-transparent rounded-full animate-spin" aria-hidden />
                ) : (
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                    <line x1="22" y1="2" x2="11" y2="13" />
                    <polygon points="22 2 15 22 11 13 2 9 22 2" />
                  </svg>
                )}
              </button>
            </div>
            <div className="flex items-center justify-between mt-3 pt-3 border-t border-[var(--border)]">
              <div className="flex items-center gap-1 flex-wrap">
                {/* Attach files deferred to later phase. */}
              </div>
              <select
                value={mode}
                onChange={(e) => setMode(e.target.value as typeof MODES[number])}
                disabled={!agentReady}
                className="bg-[var(--surface-2)] border border-[var(--border)] rounded-lg py-1.5 px-3 font-sans text-[11px] font-semibold text-[var(--text)] focus:outline-none focus:border-[rgba(0,255,136,0.3)] disabled:cursor-not-allowed disabled:opacity-70 enabled:cursor-pointer"
              >
                {MODES.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>
          </div>
        </form>
        {/* Show submit error so user knows why send didn't work */}
        {submitError && (
          <p
            className="mt-3 text-center font-sans text-[13px] text-[var(--yellow)]"
            role="alert"
          >
            {submitError}
          </p>
        )}
        {/* CTA directly under input: explains why the box is disabled and what to do */}
        {!agentReady && testStatus !== "testing" && (
          <p
            id="setup-required-msg"
            className="mt-3 text-center font-sans text-[13px] text-[var(--text-muted)]"
            role="status"
          >
            Connect an AI provider in{" "}
            <Link to="/settings?section=ai" className="text-[var(--yellow)] font-medium underline underline-offset-1 hover:opacity-90">
              Settings
            </Link>{" "}
            to get started.
          </p>
        )}
      </div>

      {/* Suggestion tiles: disabled when agent not ready */}
      <div className="w-full max-w-[680px] grid grid-cols-4 gap-2 mt-3.5 animate-[fadeUp_0.5s_0.2s_ease_both]">
        {SUGGESTIONS.map((s) => (
          <button
            key={s.label}
            type="button"
            onClick={() => fillSuggestion(s.template)}
            disabled={!agentReady}
            className="bg-[var(--surface)] border border-[var(--border)] rounded-[10px] p-3.5 flex flex-col gap-1.5 text-left transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:translate-y-0 enabled:cursor-pointer hover:border-[var(--border-hover)] hover:bg-[var(--surface-2)] hover:-translate-y-px"
          >
            <div className="text-xs font-semibold text-[var(--text)] tracking-wide">
              {s.label}
            </div>
            <div className="text-[11px] text-[var(--text-muted)] font-mono font-light leading-snug">
              {s.desc}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
