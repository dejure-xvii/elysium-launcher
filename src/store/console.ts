import { create } from "zustand";
import type { LogLine } from "../api";

export type LogLevel = "info" | "warn" | "error" | "launcher";

export interface ConsoleLine {
  id: number;
  text: string;
  level: LogLevel;
}

const MAX_LINES = 5000;
let nextId = 1;
let buffer: ConsoleLine[] = [];
let flushScheduled = false;

function levelOf(l: LogLine): LogLevel {
  const t = l.line;
  if (t.startsWith("[Launcher]")) return "launcher";
  if (/\b(ERROR|FATAL|SEVERE)\b|Exception|^\s+at /.test(t)) return "error";
  if (/\bWARN(ING)?\b/.test(t)) return "warn";
  return "info";
}

interface ConsoleStore {
  lines: ConsoleLine[];
  push: (l: LogLine) => void;
  clear: () => void;
  text: () => string;
}

export const useConsole = create<ConsoleStore>((set, get) => ({
  lines: [],

  // Lines arrive in bursts; batch them per animation frame.
  push: (l) => {
    buffer.push({ id: nextId++, text: l.line, level: levelOf(l) });
    if (flushScheduled) return;
    flushScheduled = true;
    requestAnimationFrame(() => {
      flushScheduled = false;
      const merged = get().lines.concat(buffer);
      buffer = [];
      set({ lines: merged.length > MAX_LINES ? merged.slice(-MAX_LINES) : merged });
    });
  },

  clear: () => set({ lines: [] }),

  text: () => get().lines.map((l) => l.text).join("\n"),
}));
