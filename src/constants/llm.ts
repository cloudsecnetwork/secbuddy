export type LlmProviderId = "ollama" | "openai" | "claude" | "gemini";

export const LLM_PROVIDERS: { value: LlmProviderId; label: string }[] = [
  { value: "ollama", label: "Ollama" },
  { value: "openai", label: "OpenAI" },
  { value: "claude", label: "Claude" },
  { value: "gemini", label: "Gemini" },
];

export function isProviderId(s: string): s is LlmProviderId {
  return ["ollama", "openai", "claude", "gemini"].includes(s);
}
