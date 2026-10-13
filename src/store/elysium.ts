import { create } from "zustand";
import { api, type ElysiumStatus, type ElysiumSubscription, type ElysiumUser } from "../api";

/** Elysium site account. Required to play, independent of the Minecraft accounts in `accounts.ts`. */
interface ElysiumStore {
  /** `null` until the first check finishes. */
  status: ElysiumStatus | null;
  /** Quiet session check on start (signs out if the session was revoked). */
  load: () => Promise<void>;
  /** Signs in; the status may be "inactive" (no subscription), which shows its own screen. */
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
    if (status.state !== "online" && status.state !== "inactive") throw new Error("Не удалось войти");
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

/** The subscription of the signed-in user (also while it is inactive). */
export function useElysiumSubscription(): ElysiumSubscription | null {
  return useElysium((s) =>
    s.status?.state === "online" || s.status?.state === "offline" || s.status?.state === "inactive" ? s.status.subscription : null,
  );
}

const PLAN_TITLES: Record<string, string> = {
  d7: "7 дней",
  d14: "14 дней",
  d30: "30 дней",
  d90: "90 дней",
  d180: "180 дней",
  d365: "365 дней",
  forever: "Навсегда",
  trial: "Пробный период",
  admin: "Выдана администратором",
};

export const planTitle = (plan: string | null) => (plan ? (PLAN_TITLES[plan] ?? plan) : "—");

export const dateText = (ms: number) =>
  new Date(ms).toLocaleDateString("ru-RU", { day: "2-digit", month: "2-digit", year: "numeric" });
