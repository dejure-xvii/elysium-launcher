import { create } from "zustand";
import { api, type UpdaterStatus } from "../api";
import { toast } from "./toasts";

interface UpdaterStore {
  status: UpdaterStatus | null;
  load: () => Promise<void>;
  onEvent: (s: UpdaterStatus) => void;
}

export const useUpdater = create<UpdaterStore>((set, get) => ({
  status: null,

  load: async () => set({ status: await api.launcherUpdateStatus() }),

  onEvent: (s) => {
    const prev = get().status;
    set({ status: s });
    if (prev?.stage === s.stage) return;
    // Automatic updates announce themselves; failed background checks stay quiet (Settings shows them).
    if (s.stage === "downloading") toast.info(`Устанавливается обновление лаунчера ${s.version}…`);
    if (s.stage === "waitingForGame" && prev?.stage !== "checking")
      toast.info(`Обновление лаунчера ${s.version} установится после закрытия игры`);
  },
}));
