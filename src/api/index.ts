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
  /** Ids (Modrinth project or `github:owner/repo`) of optional built-in mods that are switched off. */
  disabledMods: string[];
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

/** Elysium site account (not a Minecraft account). */
export interface ElysiumUser {
  id: string;
  username: string;
  email: string | null;
  emailVerified: boolean;
}

/** Whether the launcher may be used (Elysium account is required to play). */
export type ElysiumStatus =
  | { state: "online"; user: ElysiumUser }
  | { state: "offline"; user: ElysiumUser; lastVerifiedAt: number }
  | { state: "signedOut" }
  | { state: "unreachable" };

export type ElysiumPage = "register" | "forgot" | "account" | "privacy";

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
  modVersion: string | null;
  modUpdate: ReleaseInfo | null;
  modError: string | null;
}

export type UpdaterStage =
  | "idle"
  | "checking"
  | "upToDate"
  | "waitingForGame"
  | "downloading"
  | "installing"
  | "error";

/** Launcher auto-updater state (also sent as "launcher-update" events). */
export interface UpdaterStatus {
  stage: UpdaterStage;
  currentVersion: string;
  version: string | null;
  message: string | null;
  progress: number | null;
  checkedAt: number | null;
}

export interface BuiltinMod {
  id: string;
  name: string;
  version: string | null;
  required: boolean;
  enabled: boolean;
  installed: boolean;
  /** Mods that need this one (set for dependencies). */
  dependencyOf: string[];
  replacedByUser: boolean;
  /** Credit and project page for third-party mods from GitHub. */
  author: string | null;
  page: string | null;
}

export interface UserMod {
  file: string;
  name: string;
  version: string | null;
}

export interface ModsView {
  builtin: BuiltinMod[];
  user: UserMod[];
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

  elysiumRefresh: () => invoke<ElysiumStatus>("elysium_refresh"),
  elysiumLogin: (login: string, password: string) => invoke<ElysiumStatus>("elysium_login", { login, password }),
  elysiumLogout: () => invoke<void>("elysium_logout"),
  elysiumOpenPage: (page: ElysiumPage) => invoke<void>("elysium_open_page", { page }),

  launchGame: () => invoke<void>("launch_game"),
  reinstallFiles: () => invoke<void>("reinstall_files"),
  stopGame: () => invoke<void>("stop_game"),
  isBusy: () => invoke<boolean>("is_busy"),

  listMods: () => invoke<ModsView>("list_mods"),
  setModEnabled: (id: string, enabled: boolean) => invoke<Settings>("set_mod_enabled", { id, enabled }),

  checkUpdates: () => invoke<UpdateStatus>("check_updates"),
  updateMod: () => invoke<string>("update_mod"),
  launcherUpdateStatus: () => invoke<UpdaterStatus>("launcher_update_status"),
  /** Checks now; an update found is installed automatically. Returns its version. */
  checkLauncherUpdate: () => invoke<string | null>("check_launcher_update"),

  openGameDir: () => invoke<void>("open_game_dir"),
  openModsDir: () => invoke<void>("open_mods_dir"),
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
  onLauncherUpdate: (cb: (s: UpdaterStatus) => void): Promise<UnlistenFn> =>
    listen<UpdaterStatus>("launcher-update", (e) => cb(e.payload)),
  onAccountsChanged: (cb: () => void): Promise<UnlistenFn> =>
    listen("accounts-changed", () => cb()),
  onElysiumSignedOut: (cb: () => void): Promise<UnlistenFn> =>
    listen("elysium-signed-out", () => cb()),
};

/** Errors from Rust arrive as plain strings. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
