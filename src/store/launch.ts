import { create } from "zustand";
import { api, errorText, type GameExit, type Progress } from "../api";
import { useConsole } from "./console";
import { toast } from "./toasts";

export type Phase = "idle" | "preparing" | "running";

interface LaunchStore {
  phase: Phase;
  progress: Progress | null;
  /** Set when the game exits abnormally; drives the crash dialog. */
  crash: GameExit | null;
  launch: () => Promise<void>;
  reinstall: () => Promise<void>;
  stop: () => Promise<void>;
  onProgress: (p: Progress) => void;
  onExit: (x: GameExit) => void;
  dismissCrash: () => void;
}

export const useLaunch = create<LaunchStore>((set, get) => ({
  phase: "idle",
  progress: null,
  crash: null,

  launch: async () => {
    if (get().phase !== "idle") return;
    useConsole.getState().clear();
    set({ phase: "preparing", progress: { stage: "start", label: "Подготовка…", current: 0, total: 0 }, crash: null });
    try {
      await api.launchGame();
      set({ phase: "running", progress: null });
    } catch (e) {
      set({ phase: "idle", progress: null });
      toast.error(errorText(e));
    }
  },

  reinstall: async () => {
    if (get().phase !== "idle") return;
    set({ phase: "preparing", progress: { stage: "start", label: "Проверка файлов…", current: 0, total: 0 } });
    try {
      await api.reinstallFiles();
      toast.success("Файлы игры проверены и переустановлены");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      set({ phase: "idle", progress: null });
    }
  },

  stop: async () => {
    await api.stopGame();
  },

  onProgress: (p) => {
    if (get().phase === "preparing") set({ progress: p });
  },

  onExit: (x) => {
    set({ phase: "idle", progress: null, crash: x.crashed ? x : null });
    if (!x.crashed) toast.info("Игра закрыта");
  },

  dismissCrash: () => set({ crash: null }),
}));
