import type { LlmProviderId } from "../constants/llm";

export type ModelEntry = {
  id: string;
  name: string;
  tags: string[];
};

export type ProviderModels = {
  defaultModelId: string;
  models: ModelEntry[];
};

export type ModelsManifest = {
  version: number;
  updated: string;
  providers: Record<LlmProviderId, ProviderModels>;
};

const REMOTE_URL = "https://api.cloudsecnetwork.com/secproof/models";

/** Fetched once at app init. Non-200 or malformed response → use bundled. */
export async function fetchModelsManifest(): Promise<ModelsManifest> {
  const res = await fetch(REMOTE_URL, {
    method: "GET",
    headers: { Accept: "application/json" },
    cache: "no-store",
  });
  if (!res.ok) {
    throw new Error(`Models manifest fetch failed: ${res.status}`);
  }
  const data = await res.json();
  if (!isValidManifest(data)) {
    throw new Error("Models manifest response is malformed");
  }
  return data as ModelsManifest;
}

function isValidManifest(data: unknown): data is ModelsManifest {
  if (!data || typeof data !== "object") return false;
  const o = data as Record<string, unknown>;
  if (typeof o.version !== "number" || typeof o.updated !== "string") return false;
  if (!o.providers || typeof o.providers !== "object") return false;
  const providers = o.providers as Record<string, unknown>;
  for (const key of ["ollama", "openai", "claude", "gemini"] as const) {
    const p = providers[key];
    if (!p || typeof p !== "object") return false;
    const q = p as Record<string, unknown>;
    if (typeof q.defaultModelId !== "string") return false;
    if (!Array.isArray(q.models)) return false;
    for (const m of q.models) {
      if (!m || typeof m !== "object") return false;
      const e = m as Record<string, unknown>;
      if (typeof e.id !== "string" || typeof e.name !== "string" || !Array.isArray(e.tags)) return false;
    }
  }
  return true;
}

/** Bundled fallback when remote fetch fails. */
export const BUNDLED_MODELS_MANIFEST: ModelsManifest = {
  version: 1,
  updated: "2026-02-20",
  providers: {
    ollama: {
      defaultModelId: "llama3.2",
      models: [
        { id: "llama3.2", name: "Llama 3.2 (local)", tags: ["local"] },
      ],
    },
    openai: {
      defaultModelId: "gpt-4o-mini",
      models: [
        { id: "gpt-4o-mini", name: "GPT-4o mini", tags: ["recommended", "widely_available"] },
        { id: "gpt-4o", name: "GPT-4o", tags: ["recommended"] },
        { id: "gpt-4.1-mini", name: "GPT-4.1 mini", tags: ["recommended"] },
        { id: "gpt-4.1", name: "GPT-4.1", tags: ["recommended"] },
        { id: "gpt-5-mini", name: "GPT-5 mini (may require access)", tags: ["newer"] },
        { id: "gpt-5.2", name: "GPT-5.2 (may require access)", tags: ["newer"] },
        { id: "gpt-4-turbo", name: "GPT-4 Turbo (legacy)", tags: ["legacy"] },
        { id: "gpt-4", name: "GPT-4 (legacy)", tags: ["legacy"] },
        { id: "gpt-3.5-turbo", name: "GPT-3.5 Turbo (legacy)", tags: ["legacy"] },
      ],
    },
    claude: {
      defaultModelId: "claude-sonnet-4-6",
      models: [
        { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", tags: ["recommended"] },
        { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", tags: ["recommended", "fast"] },
        { id: "claude-opus-4-6", name: "Claude Opus 4.6", tags: ["premium"] },
        { id: "claude-opus-4-1-20250805", name: "Claude Opus 4.1 (dated)", tags: ["dated"] },
        { id: "claude-3-5-sonnet-20241022", name: "Claude 3.5 Sonnet 20241022 (legacy)", tags: ["legacy"] },
        { id: "claude-3-5-haiku-20241022", name: "Claude 3.5 Haiku 20241022 (legacy)", tags: ["legacy"] },
        { id: "claude-3-opus-20240229", name: "Claude 3 Opus 20240229 (legacy)", tags: ["legacy"] },
        { id: "claude-3-sonnet-20240229", name: "Claude 3 Sonnet 20240229 (legacy)", tags: ["legacy"] },
        { id: "claude-3-haiku-20240307", name: "Claude 3 Haiku 20240307 (legacy)", tags: ["legacy"] },
      ],
    },
    gemini: {
      defaultModelId: "gemini-2.5-flash-lite",
      models: [
        { id: "gemini-2.5-flash-lite", name: "Gemini 2.5 Flash-Lite", tags: ["recommended", "stable"] },
        { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", tags: ["recommended", "stable"] },
        { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", tags: ["premium", "stable"] },
        { id: "gemini-flash-lite-latest", name: "Gemini Flash-Lite Latest", tags: ["alias", "moving_target"] },
        { id: "gemini-flash-latest", name: "Gemini Flash Latest", tags: ["alias", "moving_target"] },
        { id: "gemini-pro-latest", name: "Gemini Pro Latest", tags: ["alias", "moving_target"] },
        { id: "gemini-2.0-flash", name: "Gemini 2.0 Flash (legacy)", tags: ["legacy"] },
        { id: "gemini-2.0-flash-001", name: "Gemini 2.0 Flash 001 (legacy)", tags: ["legacy"] },
        { id: "gemini-2.0-flash-lite", name: "Gemini 2.0 Flash-Lite (legacy)", tags: ["legacy"] },
        { id: "gemini-2.0-flash-lite-001", name: "Gemini 2.0 Flash-Lite 001 (legacy)", tags: ["legacy"] },
      ],
    },
  },
};
