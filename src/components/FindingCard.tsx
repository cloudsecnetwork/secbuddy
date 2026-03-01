import type { Finding } from "../stores/chat";

type Props = { finding: Finding };

const severityConfig: Record<
  string,
  { bg: string; borderColor: string; severityColor: string }
> = {
  critical: {
    bg: "bg-[var(--red-dim)]",
    borderColor: "var(--red)",
    severityColor: "text-[var(--red)]",
  },
  high: {
    bg: "bg-[var(--orange-dim)]",
    borderColor: "var(--orange)",
    severityColor: "text-[var(--orange)]",
  },
  medium: {
    bg: "bg-[var(--yellow-dim)]",
    borderColor: "var(--yellow)",
    severityColor: "text-[var(--yellow)]",
  },
  low: {
    bg: "bg-[var(--blue-dim)]",
    borderColor: "var(--blue)",
    severityColor: "text-[var(--blue)]",
  },
  info: {
    bg: "bg-[var(--surface-2)]",
    borderColor: "var(--text-dim)",
    severityColor: "text-[var(--text-dim)]",
  },
};

export function FindingCard({ finding }: Props) {
  const key = finding.severity.toLowerCase();
  const config = severityConfig[key] ?? severityConfig.info;

  const tags: string[] = [];
  if (finding.owaspRef) tags.push(finding.owaspRef);
  if (finding.cweRef) tags.push(finding.cweRef);
  if (finding.mitreRef) tags.push(finding.mitreRef);

  return (
    <div
      className={`rounded-[9px] py-3 px-3.5 mb-2 ${config.bg}`}
      style={{ borderLeft: `3px solid ${config.borderColor}` }}
    >
      <div className="flex items-center gap-2 mb-1.5">
        <span
          className={`font-mono text-[10px] font-semibold uppercase tracking-wider ${config.severityColor}`}
        >
          {finding.severity}
        </span>
        <span className="text-[13px] font-bold text-[var(--text)]">
          {finding.title}
        </span>
      </div>
      <p className="text-[12px] text-[var(--text-muted)] leading-relaxed mb-2">
        {finding.description}
      </p>
      {tags.length > 0 && (
        <div className="flex gap-1.5 flex-wrap mb-0">
          {tags.map((t) => (
            <span
              key={t}
              className="font-mono text-[10px] py-0.5 px-1.5 rounded border bg-black/5 text-[var(--text-dim)] border-[var(--border)]"
            >
              {t}
            </span>
          ))}
        </div>
      )}
      {finding.recommendedAction && (
        <div className="mt-2 pt-2 border-t border-white/5 text-[12px] text-[var(--text-muted)]">
          <strong className="text-[var(--accent)] font-semibold">Next:</strong>{" "}
          {finding.recommendedAction}
        </div>
      )}
    </div>
  );
}
