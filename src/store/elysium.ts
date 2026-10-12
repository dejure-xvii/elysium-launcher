import { create } from "zustand";
import { api, type ElysiumStatus, type ElysiumUser } from "../api";

/** Elysium site account. Required to play, independent of the Minecraft accounts in `accounts.ts`. */
interface ElysiumStore {
  /** `null` until the first check finishes. */
  status: ElysiumStatus | null;
  /** Quiet session check on start (signs out if the session was revoked). */
  load: () => Promise<void>;
  login: (login: string, password: string) => Promise<ElysiumUser>;
  logout: () => Promise<void>;
  /** The backend found the session revoked (e.g. while starting the game). */
  signedOut: () => void;
}

export const useElysium = create<ElysiumStore>((set) => ({
  status: null,

  load: async () => {
    try {
      set({ status: await api.elysiumRefresh() });
    } catch {
      set({ status: { state: "unreachable" } });
    }
  },

  login: async (login, password) => {
    const status = await api.elysiumLogin(login, password);
    set({ status });
    if (status.state !== "online") throw new Error("Не удалось войти");
    return status.user;
  },

  logout: async () => {
    await api.elysiumLogout();
    set({ status: { state: "signedOut" } });
  },

  signedOut: () => set({ status: { state: "signedOut" } }),
}));

/** The signed-in Elysium user, when the launcher may be used. */
export function useElysiumUser(): ElysiumUser | null {
  return useElysium((s) => (s.status?.state === "online" || s.status?.state === "offline" ? s.status.user : null));
}
