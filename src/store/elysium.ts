import { create } from "zustand";
import { api, type ElysiumUser } from "../api";

/** Elysium site account. Optional and independent of the Minecraft accounts in `accounts.ts`. */
interface ElysiumStore {
  user: ElysiumUser | null;
  dialogOpen: boolean;
  /** Cached profile first, then a server check (signs out if the session was revoked). */
  load: () => Promise<void>;
  login: (login: string, password: string) => Promise<ElysiumUser>;
  logout: () => Promise<void>;
  openDialog: () => void;
  closeDialog: () => void;
}

export const useElysium = create<ElysiumStore>((set) => ({
  user: null,
  dialogOpen: false,

  load: async () => {
    set({ user: await api.elysiumAccount() });
    set({ user: await api.elysiumRefresh() });
  },

  login: async (login, password) => {
    const user = await api.elysiumLogin(login, password);
    set({ user });
    return user;
  },

  logout: async () => {
    await api.elysiumLogout();
    set({ user: null });
  },

  openDialog: () => set({ dialogOpen: true }),
  closeDialog: () => set({ dialogOpen: false }),
}));
