import { useState, useEffect } from "react";
import type { ToolInvocation } from "../stores/chat";
import { recordApprovalAndExecute, cancelToolInvocation } from "../lib/tauri";

type Props = { inv: ToolInvocation };

function riskCategoryLabel(category: string | null): string {
  if (category === "high_impact") return "High impact";
  if (category === "active") return "Active tool";
  if (category === "passive") return "Passive tool";
  return category ?? "";
}

function riskCategoryExplainer(category: string | null): string | null {
  if (category === "passive") return "Views and analyzes systems without making changes.";
  if (category === "active") return "May send requests or make limited system changes.";
  if (category === "high_impact") return "Can make significant, sensitive, or irreversible changes.";
  return null;
}

const RUNNING_ELAPSED_INTERVAL_MS = 1000;

export function ToolCard({ inv }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [approving, setApproving] = useState(false);
  const [decisionSent, setDecisionSent] = useState<null | "approved" | "dry_run">(null);
  const [runningElapsedSec, setRunningElapsedSec] = useState(0);
  const [stopping, setStopping] = useState(false);
  const isPending = inv.status === "pending";
  const isRunning = inv.status === "running";

  // Live elapsed seconds while running (increments every 1s)
  useEffect(() => {
    if (!isRunning) return;
    const startedAt = inv.runningStartedAt ?? inv.createdAt;
    const tick = () =>
      setRunningElapsedSec(Math.floor((Date.now() - startedAt) / RUNNING_ELAPSED_INTERVAL_MS));
    tick();
    const id = setInterval(tick, RUNNING_ELAPSED_INTERVAL_MS);
    return () => clearInterval(id);
  }, [isRunning, inv.runningStartedAt, inv.createdAt]);

  const isSkipped = inv.status === "complete" && inv.durationMs === 0;
  const isInterrupted =
    inv.status === "failed" &&
    inv.rawOutput?.trim() === "Interrupted by app restart.";
  const isCancelled =
    inv.status === "failed" &&
    inv.rawOutput?.trim() === "Cancelled by user.";
  const statusDot =
    inv.status === "complete"
      ? isSkipped
        ? "bg-[var(--text-muted)]"
        : "bg-[var(--accent)] shadow-[0_0_5px_var(--accent)]"
      : isInterrupted || isCancelled
        ? "bg-[var(--orange)]"
        : inv.status === "denied" || inv.status === "failed"
          ? "bg-[var(--red)]"
          : inv.status === "running"
          ? "bg-[var(--blue)] shadow-[0_0_6px_var(--blue)] animate-pulse"
          : isPending
            ? "bg-[var(--orange)] shadow-[0_0_5px_var(--orange)] animate-pulse"
            : "bg-[var(--text-dim)]";

  const fullCommand = [inv.toolName, inv.inputParams, inv.target]
    .filter(Boolean)
    .join(" ")
    .trim() || inv.toolName || inv.id;
  const impactLabel = inv.riskCategory ? riskCategoryLabel(inv.riskCategory) : null;
  const explainer = riskCategoryExplainer(inv.riskCategory);

  // User-facing status label (header + approval strip)
  const statusLabel =
    inv.status === "running"
      ? `Running (${runningElapsedSec}s)`
      : isInterrupted
        ? "Interrupted"
        : isCancelled
          ? "Cancelled"
          : inv.status === "denied" || inv.status === "failed"
            ? "Denied"
          : inv.status === "complete"
          ? inv.durationMs === 0
            ? "Skipped"
            : "Completed"
          : isPending && decisionSent
            ? decisionSent === "approved"
              ? "Accepted"
              : "Skipped"
            : null;

  const handleDecision = async (decision: "approved" | "dry_run") => {
    if (approving) return;
    setApproving(true);
    setDecisionSent(decision);
    try {
      await recordApprovalAndExecute(inv.id, decision);
    } catch (e) {
      console.error(e);
      setApproving(false);
      setDecisionSent(null);
    }
  };

  return (
    <div
      className={`bg-[var(--surface)] border rounded-[10px] mb-2 overflow-hidden transition-[border-color] hover:border-[var(--border-hover)] ${
        isPending
          ? "border-[var(--orange)]/40"
          : expanded
            ? "border-[var(--border-hover)]"
            : "border-[var(--border)]"
      }`}
    >
      <button
        type="button"
        className="w-full flex items-center gap-2.5 py-2.5 px-3.5 text-left cursor-pointer select-none"
        onClick={() => setExpanded((e) => !e)}
      >
        <span className={`w-[7px] h-[7px] rounded-full shrink-0 ${statusDot}`} />
        <div className="flex-1 min-w-0 flex flex-col gap-0.5">
          <span
            className="font-mono text-[12px] font-medium text-[var(--text)] truncate block"
            title={fullCommand}
          >
            {fullCommand}
          </span>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {statusLabel && (
            <span
              className={`font-sans text-[10px] font-semibold uppercase tracking-wide ${
                statusLabel === "Accepted" || statusLabel === "Completed"
                  ? "text-[var(--accent)]"
                  : statusLabel === "Skipped"
                    ? "text-[var(--text-muted)]"
                    : statusLabel === "Interrupted"
                      ? "text-[var(--orange)]"
                      : statusLabel === "Cancelled"
                      ? "text-[var(--orange)]"
                      : statusLabel === "Denied"
                        ? "text-[var(--red)]"
                        : statusLabel.startsWith("Running")
                          ? "text-[var(--blue)]"
                          : "text-[var(--text-dim)]"
              }`}
            >
              {statusLabel}
            </span>
          )}
          {isRunning && (
            <button
              type="button"
              disabled={stopping}
              onClick={(e) => {
                e.stopPropagation();
                setStopping(true);
                cancelToolInvocation(inv.id).catch(() => setStopping(false));
              }}
              className="shrink-0 py-1 px-2 rounded border border-[var(--red)]/50 font-sans text-[10px] font-semibold text-[var(--red)] hover:bg-[var(--red)]/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Stop
            </button>
          )}
          {impactLabel && !statusLabel && (
            <span className="font-mono text-[10px] text-[var(--text-dim)]">
              {impactLabel}
            </span>
          )}
          {inv.durationMs != null && inv.durationMs > 0 && (
            <span className="font-mono text-[10px] text-[var(--text-dim)]">
              {inv.durationMs}ms
            </span>
          )}
          <span className="text-[10px] text-[var(--text-dim)]">
            {expanded ? "\u25B2" : "\u25BC"}
          </span>
        </div>
      </button>

      {explainer && (
        <div className="px-3.5 pb-2.5 pt-0">
          <p className="text-[11px] text-[var(--text-muted)] leading-relaxed">
            {explainer}
          </p>
        </div>
      )}

      {isPending && (
        <div className="border-t border-[var(--orange)]/20 bg-[var(--orange-dim)] px-3.5 py-2.5 flex items-center justify-between gap-3">
          {decisionSent ? (
            <div className="flex items-center gap-2 min-w-0">
              <span
                className={`font-sans text-[12px] font-semibold ${
                  decisionSent === "approved"
                    ? "text-[var(--accent)]"
                    : "text-[var(--text-muted)]"
                }`}
              >
                {decisionSent === "approved" ? "Accepted" : "Skipped"}
              </span>
              <span className="text-[var(--text-muted)] text-[11px]">
                {decisionSent === "approved"
                  ? "Running when you’re done with the rest."
                  : "Won’t run."}
              </span>
            </div>
          ) : (
            <>
              <div className="flex items-center gap-2 min-w-0">
                <span className="font-mono text-[11px] text-[var(--orange)] font-medium shrink-0">
                  Approval required
                </span>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  disabled={approving}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDecision("dry_run");
                  }}
                  className="py-1.5 px-3 rounded-md border border-[var(--border)] bg-transparent font-sans text-[12px] font-semibold cursor-pointer transition-all text-[var(--text-muted)] hover:border-[var(--border-hover)] hover:text-[var(--text)] disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Skip
                </button>
                <button
                  type="button"
                  disabled={approving}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDecision("approved");
                  }}
                  className="py-1.5 px-3 rounded-md border font-sans text-[12px] font-semibold cursor-pointer transition-all bg-[var(--accent-dim)] text-[var(--accent)] border-[rgba(0,255,136,0.3)] hover:bg-[rgba(0,255,136,0.2)] disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Accept
                </button>
              </div>
            </>
          )}
        </div>
      )}

      {expanded && (
        <div className="border-t border-[var(--border)] bg-[#0d0d0f] p-3.5">
          <pre className="font-mono text-[11.5px] text-[#a0ffcc] leading-relaxed whitespace-pre-wrap max-h-[280px] overflow-y-auto">
            {inv.rawOutput ?? "(no output)"}
          </pre>
        </div>
      )}
    </div>
  );
}
