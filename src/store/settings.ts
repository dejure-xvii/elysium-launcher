import { create } from "zustand";
import { api, errorText, type Settings, type SystemInfo } from "../api";
import { toast } from "./toasts";

interface SettingsStore {
  settings: Settings | null;
  system: SystemInfo | null;
  load: () => Promise<void>;
  /** Applies a change locally right away and persists it (debounced). */
  update: (patch: Partial<Settings>) => void;
  /** Re-reads settings after the backend changed them (e.g. selected account). */
  refresh: () => Promise<void>;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;

export const useSettings = create<SettingsStore>((set, get) => ({
  settings: null,
  system: null,

  load: async () => {
    const [settings, system] = await Promise.all([api.getSettings(), api.systemInfo()]);
    set({ settings, system });
  },

  refresh: async () => {
    set({ settings: await api.getSettings() });
  },

  update: (patch) => {
    const current = get().settings;
    if (!current) return;
    const next = { ...current, ...patch };
    set({ settings: next });
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        set({ settings: await api.setSettings(get().settings ?? next) });
      } catch (e) {
        toast.error(errorText(e));
      }
    }, 400);
  },
}));

export function formatGb(mb: number): string {
  const gb = mb / 1024;
  return `${Number.isInteger(gb) ? gb : gb.toFixed(1)} ГБ`;
}
