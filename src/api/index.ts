// Typed bridge to the Rust backend (commands + events).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type AfterLaunch = "minimize" | "close" | "keep";

export interface Settings {
  memoryMb: number;
  width: number;
  height: number;
  fullscreen: boolean;
  gameDir: string;
  jvmArgs: string;
  afterLaunch: AfterLaunch;
  selectedAccount: string | null;
}

export interface SystemInfo {
  totalMemoryMb: number;
  maxMemoryMb: number;
  recommendedMemoryMb: number;
  launcherVersion: string;
  msLoginAvailable: boolean;
}

export interface Account {
  id: string;
  kind: "microsoft" | "offline";
  username: string;
  uuid: string;
  skinUrl?: string | null;
}

export interface DeviceCode {
  userCode: string;
  deviceCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export interface Progress {
  stage: string;
  label: string;
  current: number;
  total: number;
}

export interface LogLine {
  line: string;
  stderr: boolean;
}

export interface GameExit {
  code: number | null;
  crashed: boolean;
  crashReport: string | null;
  logFile: string | null;
}

export interface ReleaseInfo {
  version: string;
  notes: string;
  page: string;
}

export interface UpdateStatus {
  launcherVersion: string;
  modVersion: string | null;
  launcherUpdate: ReleaseInfo | null;
  modUpdate: ReleaseInfo | null;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  systemInfo: () => invoke<SystemInfo>("system_info"),

  listAccounts: () => invoke<Account[]>("list_accounts"),
  addOfflineAccount: (username: string) => invoke<Account>("add_offline_account", { username }),
  removeAccount: (id: string) => invoke<void>("remove_account", { id }),
  selectAccount: (id: string) => invoke<void>("select_account", { id }),
  msLoginStart: () => invoke<DeviceCode>("ms_login_start"),
  msLoginFinish: () => invoke<Account>("ms_login_finish"),
  msLoginCancel: () => invoke<void>("ms_login_cancel"),

  launchGame: () => invoke<void>("launch_game"),
  reinstallFiles: () => invoke<void>("reinstall_files"),
  stopGame: () => invoke<void>("stop_game"),
  isBusy: () => invoke<boolean>("is_busy"),

  checkUpdates: () => invoke<UpdateStatus>("check_updates"),
  updateMod: () => invoke<string>("update_mod"),
  updateLauncher: () => invoke<void>("update_launcher"),

  openGameDir: () => invoke<void>("open_game_dir"),
  openLogsDir: () => invoke<void>("open_logs_dir"),
  openFile: (path: string) => invoke<void>("open_file", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

export const events = {
  onProgress: (cb: (p: Progress) => void): Promise<UnlistenFn> =>
    listen<Progress>("launch-progress", (e) => cb(e.payload)),
  onLog: (cb: (l: LogLine) => void): Promise<UnlistenFn> =>
    listen<LogLine>("game-log", (e) => cb(e.payload)),
  onExit: (cb: (x: GameExit) => void): Promise<UnlistenFn> =>
    listen<GameExit>("game-exit", (e) => cb(e.payload)),
  onAccountsChanged: (cb: () => void): Promise<UnlistenFn> =>
    listen("accounts-changed", () => cb()),
};

/** Errors from Rust arrive as plain strings. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
