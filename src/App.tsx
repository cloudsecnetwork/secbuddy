import React, { useEffect } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { Home } from "./routes/Home";
import { Chat } from "./routes/Chat";
import { Settings } from "./routes/Settings";
import { useModelsConfigStore } from "./stores/modelsConfig";
import "./index.css";

class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { hasError: boolean; error: Error | null }
> {
  override state = { hasError: false, error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  override render() {
    if (this.state.hasError && this.state.error) {
      return (
        <div style={{ padding: 24, color: "#ff4466", fontFamily: "system-ui", maxWidth: 600 }}>
          <h2>Something went wrong</h2>
          <pre style={{ whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
            {this.state.error.message}
          </pre>
          <button
            type="button"
            onClick={() => window.location.reload()}
            style={{
              marginTop: 16,
              padding: "8px 16px",
              cursor: "pointer",
              background: "var(--surface-2, #333)",
              color: "var(--text, #eee)",
              border: "1px solid var(--border, #555)",
              borderRadius: 6,
            }}
          >
            Reload app
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

function Layout() {
  const loadFromRemote = useModelsConfigStore((s) => s.loadFromRemote);
  useEffect(() => {
    loadFromRemote();
  }, [loadFromRemote]);

  return (
    <div className="flex h-screen bg-[var(--bg)] text-[var(--text)] relative z-[1]">
      <Sidebar />
      <main className="flex-1 flex flex-col min-w-0 min-h-0">
        <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/chat/:chatId" element={<Chat />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </div>
        <StatusBar />
      </main>
    </div>
  );
}

export default function App() {
  return (
    <ErrorBoundary>
      <BrowserRouter>
        <Layout />
      </BrowserRouter>
    </ErrorBoundary>
  );
}
