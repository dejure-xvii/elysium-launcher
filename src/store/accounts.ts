import { create } from "zustand";
import { api, type Account } from "../api";
import { useSettings } from "./settings";

interface AccountsStore {
  accounts: Account[];
  load: () => Promise<void>;
  addOffline: (username: string) => Promise<Account>;
  remove: (id: string) => Promise<void>;
  select: (id: string) => Promise<void>;
  /** Called after a Microsoft sign-in completes. */
  added: (account: Account) => Promise<void>;
}

export const useAccounts = create<AccountsStore>((set, get) => ({
  accounts: [],

  load: async () => set({ accounts: await api.listAccounts() }),

  addOffline: async (username) => {
    const account = await api.addOfflineAccount(username);
    await get().added(account);
    return account;
  },

  added: async () => {
    await get().load();
    await useSettings.getState().refresh();
  },

  remove: async (id) => {
    await api.removeAccount(id);
    await get().load();
    await useSettings.getState().refresh();
  },

  select: async (id) => {
    await api.selectAccount(id);
    await useSettings.getState().refresh();
  },
}));

/** Selected account, falling back to the first one (same rule as the backend). */
export function useSelectedAccount(): Account | undefined {
  const accounts = useAccounts((s) => s.accounts);
  const selected = useSettings((s) => s.settings?.selectedAccount);
  return accounts.find((a) => a.id === selected) ?? accounts[0];
}

export const NICK_RE = /^[A-Za-z0-9_]{3,16}$/;
