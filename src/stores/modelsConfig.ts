import { create } from "zustand";
import type { ModelsManifest } from "../data/models-manifest";
import { BUNDLED_MODELS_MANIFEST, fetchModelsManifest } from "../data/models-manifest";

type ModelsConfigState = {
  manifest: ModelsManifest;
  loadFromRemote: () => Promise<void>;
};

export const useModelsConfigStore = create<ModelsConfigState>((set) => ({
  manifest: BUNDLED_MODELS_MANIFEST,

  loadFromRemote: async () => {
    try {
      const manifest = await fetchModelsManifest();
      set({ manifest });
    } catch {
      set({ manifest: BUNDLED_MODELS_MANIFEST });
    }
  },
}));
