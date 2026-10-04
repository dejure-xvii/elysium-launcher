import { create } from "zustand";

export type Page = "play" | "mods" | "settings" | "console" | "accounts";

interface NavStore {
  page: Page;
  go: (page: Page) => void;
}

export const useNav = create<NavStore>((set) => ({
  page: "play",
  go: (page) => set({ page }),
}));
