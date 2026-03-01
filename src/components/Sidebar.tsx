import { Link, useLocation } from "react-router-dom";
import { useEffect, useState } from "react";
import { listChats } from "../lib/tauri";
import { useSettingsStore } from "../stores/settings";

export function Sidebar() {
  const location = useLocation();
  const tools = useSettingsStore((s) => s.tools);
  const load = useSettingsStore((s) => s.load);
  const [chats, setChats] = useState<Array<{ id: string; title: string; updatedAt: number }>>([]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    let cancelled = false;
    listChats()
      .then((rows) => {
        if (cancelled) return;
        const list = rows.map(([id, title, , updatedAt]) => ({ id, title, updatedAt }));
        list.sort((a, b) => b.updatedAt - a.updatedAt);
        setChats(list);
      })
      .catch((e) => console.error("listChats failed", e));
    return () => {
      cancelled = true;
    };
  }, [location.pathname, chats.length]);

  const availableCount = tools.filter((t) => t.available).length;

  return (
    <aside className="w-60 min-w-[240px] bg-[var(--surface)] border-r border-[var(--border)] flex flex-col relative z-10">
      <div className="py-5 px-[18px] pb-4 border-b border-[var(--border)]">
        <div className="mb-4">
          <span className="text-base font-bold tracking-wide text-[var(--text)]">
            SecBuddy
          </span>
        </div>
        <p className="text-[10px] font-semibold tracking-[0.12em] uppercase text-[var(--text-dim)]">
          {availableCount} tools available
        </p>
        <Link
          to="/"
          className="mt-3 w-full flex items-center justify-center gap-2 bg-[var(--accent-dim)] border border-[rgba(0,255,136,0.2)] text-[var(--accent)] font-sans text-[13px] font-semibold py-2.5 px-3.5 rounded-lg cursor-pointer transition-all duration-150 tracking-wide hover:bg-[rgba(0,255,136,0.15)] hover:border-[rgba(0,255,136,0.4)]"
        >
          <span>+</span> New Session
        </Link>
      </div>

      <div className="flex-1 overflow-y-auto py-4 px-3 pt-4">
        <div className="text-[10px] font-semibold tracking-[0.12em] text-[var(--text-dim)] uppercase px-1.5 mb-1.5">
          Recent
        </div>
        {chats.slice(0, 15).map((c) => (
          <Link
            key={c.id}
            to={`/chat/${c.id}`}
            className={`flex items-center py-2 px-2.5 rounded-md transition-all duration-120 mb-px ${
              location.pathname === `/chat/${c.id}`
                ? "bg-[var(--surface-2)] border border-[var(--border)]"
                : "hover:bg-[var(--surface-2)]"
            }`}
          >
            <span
              className={`text-xs font-mono truncate ${
                location.pathname === `/chat/${c.id}`
                  ? "text-[var(--text)]"
                  : "text-[var(--text-muted)]"
              }`}
            >
              {c.title || "Untitled"}
            </span>
          </Link>
        ))}
      </div>

      <div className="border-t border-[var(--border)] py-3 px-3">
        <Link
          to="/settings"
          className="flex items-center gap-2 py-2 px-2.5 rounded-md text-[var(--text-muted)] text-xs font-mono hover:bg-[var(--surface-2)] hover:text-[var(--text)] transition-all"
        >
          ⚙ Settings
        </Link>
      </div>
    </aside>
  );
}
