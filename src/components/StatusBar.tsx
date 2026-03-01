import { Link } from "react-router-dom";
import { useSettingsStore } from "../stores/settings";
import { LLM_PROVIDERS, isProviderId } from "../constants/llm";

export function StatusBar() {
  const testStatus = useSettingsStore((s) => s.testStatus);
  const llmProvider = useSettingsStore((s) => s.llmProvider);
  const llmModel = useSettingsStore((s) => s.llmModel);

  const providerLabel =
    (isProviderId(llmProvider) && LLM_PROVIDERS.find((p) => p.value === llmProvider)?.label) ||
    llmProvider;

  return (
    <div
      className="flex items-center justify-between gap-4 py-2.5 px-5 border-t border-[var(--border)] bg-[var(--surface)] flex-shrink-0 font-mono text-[10.5px]"
      role="status"
    >
      <div className="flex items-center gap-1.5">
        {testStatus === "ok" && (
          <>
            <span
              className="w-1.5 h-1.5 rounded-full bg-[var(--accent)] animate-pulse"
              style={{ boxShadow: "0 0 5px var(--accent)" }}
              aria-hidden
            />
            <span className="text-[var(--accent)]">Agent ready</span>
            <span className="text-[var(--text-muted)]" aria-label={`Provider: ${providerLabel}, Model: ${llmModel}`}>
              <span className="mx-1.5" aria-hidden>
                ·
              </span>
              {providerLabel}
              <span className="mx-1" aria-hidden>·</span>
              <span className="truncate max-w-[120px] inline-block align-bottom" title={llmModel}>{llmModel}</span>
            </span>
          </>
        )}
        {testStatus === "testing" && (
          <>
            <span
              className="w-1.5 h-1.5 rounded-full bg-[var(--yellow)] animate-pulse"
              aria-hidden
            />
            <span className="text-[var(--text-muted)]">Connecting…</span>
          </>
        )}
        {(testStatus === "idle" || testStatus === "error") && (
          <>
            <span
              className="w-1.5 h-1.5 rounded-full bg-[var(--yellow)]"
              style={{ boxShadow: "0 0 5px var(--yellow-dim)" }}
              aria-hidden
            />
            <span className="text-[var(--yellow)]">Not connected</span>
            <Link
              to="/settings?section=ai"
              className="ml-1.5 text-[var(--yellow)] font-medium hover:opacity-90 underline underline-offset-1"
            >
              Open Settings
            </Link>
          </>
        )}
      </div>
    </div>
  );
}
