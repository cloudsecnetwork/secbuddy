export type LlmProviderId = "ollama" | "openai" | "claude" | "gemini";

/** Default Ollama HTTP API root (no trailing slash). */
export const OLLAMA_DEFAULT_BASE_URL = "http://localhost:11434";

/** Base URL stored for new / switched provider: local root for Ollama, empty for cloud (SDK default hosts). */
export function defaultLlmBaseUrl(provider: LlmProviderId): string {
  return provider === "ollama" ? OLLAMA_DEFAULT_BASE_URL : "";
}

export function isLikelyOllamaDefaultBaseUrl(url: string): boolean {
  const r = url.trim().replace(/\/+$/, "").toLowerCase();
  return r === "http://localhost:11434" || r === "http://127.0.0.1:11434";
}

/** DB → UI: Ollama gets a non-empty default; cloud providers never inherit Ollama URL. */
export function coalesceLlmBaseUrlForProvider(
  provider: string,
  baseFromDb: string | null | undefined
): string {
  const p: LlmProviderId = isProviderId(provider) ? provider : "ollama";
  const raw = baseFromDb?.trim() ?? "";
  if (p === "ollama") {
    return raw || OLLAMA_DEFAULT_BASE_URL;
  }
  if (raw === "" || isLikelyOllamaDefaultBaseUrl(raw)) {
    return "";
  }
  return raw;
}

export const LLM_PROVIDERS: { value: LlmProviderId; label: string }[] = [
  { value: "ollama", label: "Ollama" },
  { value: "openai", label: "OpenAI" },
  { value: "claude", label: "Claude" },
  { value: "gemini", label: "Gemini" },
];

export function isProviderId(s: string): s is LlmProviderId {
  return ["ollama", "openai", "claude", "gemini"].includes(s);
}
