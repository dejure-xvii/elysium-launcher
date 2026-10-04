import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowDownToLine, FolderOpen, RefreshCw, RotateCcw, Wrench } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorText, type AfterLaunch, type UpdaterStatus, type UpdateStatus } from "../api";
import { RamSlider } from "../components/RamSlider";
import { useLaunch } from "../store/launch";
import { useSettings } from "../store/settings";
import { toast } from "../store/toasts";
import { useUpdater } from "../store/updater";

const ABSOLUTE_PATH = /^([A-Za-z]:[\\/]|\\\\)/;

const RESOLUTIONS: [number, number][] = [
  [854, 480],
  [1280, 720],
  [1600, 900],
  [1920, 1080],
  [2560, 1440],
];

const AFTER_LAUNCH: { value: AfterLaunch; label: string }[] = [
  { value: "minimize", label: "Свернуть" },
  { value: "close", label: "Закрыть" },
  { value: "keep", label: "Оставить открытым" },
];

function Section({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="section glass">
      <div className="section-head">
        <h3>{title}</h3>
        {hint && <span className="muted small">{hint}</span>}
      </div>
      {children}
    </section>
  );
}

export function SettingsPage() {
  const { settings, system, update } = useSettings();
  const phase = useLaunch((s) => s.phase);
  const reinstall = useLaunch((s) => s.reinstall);

  if (!settings || !system) return null;

  const resKey = `${settings.width}x${settings.height}`;
  const isPreset = RESOLUTIONS.some(([w, h]) => `${w}x${h}` === resKey);

  const pickFolder = async () => {
    const dir = await openDialog({ directory: true, defaultPath: settings.gameDir, title: "Папка игры" });
    if (typeof dir === "string") update({ gameDir: dir });
  };

  return (
    <div className="page">
      <h2 className="page-title">Настройки</h2>

      <Section title="Оперативная память" hint="Сколько памяти получит игра">
        <RamSlider
          value={settings.memoryMb}
          max={system.maxMemoryMb}
          recommended={system.recommendedMemoryMb}
          total={system.totalMemoryMb}
          onChange={(memoryMb) => update({ memoryMb })}
        />
      </Section>

      <Section title="Окно игры">
        <div className="row">
          <label className="field">
            <span>Разрешение</span>
            <select
              className="input"
              value={isPreset ? resKey : "custom"}
              disabled={settings.fullscreen}
              onChange={(e) => {
                if (e.target.value === "custom") return;
                const [w, h] = e.target.value.split("x").map(Number);
                update({ width: w, height: h });
              }}
            >
              {RESOLUTIONS.map(([w, h]) => (
                <option key={`${w}x${h}`} value={`${w}x${h}`}>
                  {w} × {h}
                </option>
              ))}
              <option value="custom">Своё</option>
            </select>
          </label>
          <label className="field narrow">
            <span>Ширина</span>
            <input
              className="input"
              type="number"
              min={320}
              value={settings.width}
              disabled={settings.fullscreen}
              onChange={(e) => update({ width: Number(e.target.value) || 0 })}
            />
          </label>
          <label className="field narrow">
            <span>Высота</span>
            <input
              className="input"
              type="number"
              min={240}
              value={settings.height}
              disabled={settings.fullscreen}
              onChange={(e) => update({ height: Number(e.target.value) || 0 })}
            />
          </label>
          <label className="toggle">
            <input
              type="checkbox"
              checked={settings.fullscreen}
              onChange={(e) => update({ fullscreen: e.target.checked })}
            />
            <span className="toggle-track">
              <span className="toggle-thumb" />
            </span>
            Полноэкранный режим
          </label>
        </div>
      </Section>

      <Section title="После запуска игры">
        <div className="segmented">
          {AFTER_LAUNCH.map((o) => (
            <button
              key={o.value}
              className={settings.afterLaunch === o.value ? "active" : ""}
              onClick={() => update({ afterLaunch: o.value })}
            >
              {o.label}
            </button>
          ))}
        </div>
      </Section>

      <Section title="Папка игры" hint="Миры, моды, ресурспаки и логи">
        <div className="row">
          <input
            className={`input grow mono ${ABSOLUTE_PATH.test(settings.gameDir) ? "" : "invalid"}`}
            title={ABSOLUTE_PATH.test(settings.gameDir) ? undefined : "Укажите полный путь, например C:\\Games\\.elysium"}
            value={settings.gameDir}
            onChange={(e) => update({ gameDir: e.target.value })}
            spellCheck={false}
          />
          <button className="btn btn-ghost" onClick={pickFolder}>
            Обзор…
          </button>
        </div>
        <div className="row">
          <button className="btn btn-ghost" onClick={() => api.openGameDir().catch((e) => toast.error(errorText(e)))}>
            <FolderOpen size={16} /> Открыть папку игры
          </button>
          <button className="btn btn-ghost" disabled={phase !== "idle"} onClick={reinstall}>
            <Wrench size={16} /> Переустановить файлы
          </button>
        </div>
      </Section>

      <Section title="Аргументы JVM" hint="Для опытных пользователей">
        <textarea
          className="input mono"
          rows={2}
          placeholder="-XX:+UseZGC -XX:+ZGenerational"
          value={settings.jvmArgs}
          onChange={(e) => update({ jvmArgs: e.target.value })}
          spellCheck={false}
        />
      </Section>

      <UpdatesSection />
    </div>
  );
}

function timeOf(ms: number): string {
  return new Date(ms).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
}

/** One line about what the launcher updater is doing. */
function launcherStatusText(s: UpdaterStatus | null): string {
  if (!s) return "";
  switch (s.stage) {
    case "checking":
      return "Проверка обновлений…";
    case "upToDate":
      return s.checkedAt ? `Последняя версия · проверено в ${timeOf(s.checkedAt)}` : "Последняя версия";
    case "waitingForGame":
      return `Найдена версия ${s.version} — установится после закрытия игры`;
    case "downloading":
      return `Скачивается ${s.version}` + (s.progress != null ? ` — ${Math.round(s.progress * 100)}%` : "…");
    case "installing":
      return `Устанавливается ${s.version}, лаунчер перезапустится`;
    case "error":
      return s.message ?? "Ошибка обновления";
    default:
      return "Обновления проверяются автоматически";
  }
}

function UpdatesSection() {
  const updater = useUpdater((s) => s.status);
  const [mod, setMod] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState<"check" | "mod" | null>(null);
  const launcherBusy = updater != null && ["checking", "downloading", "installing"].includes(updater.stage);

  const checkMod = async () => {
    const s = await api.checkUpdates();
    setMod(s);
    return s;
  };

  useEffect(() => {
    checkMod().catch(() => {});
  }, []);

  const check = async () => {
    setBusy("check");
    try {
      const [found, m] = await Promise.all([api.checkLauncherUpdate(), checkMod()]);
      if (found) toast.info(`Найдено обновление лаунчера ${found} — устанавливается автоматически`);
      if (m.modError) toast.error(m.modError);
      if (!found && !m.modUpdate && !m.modError) toast.success("У вас последние версии");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const updateMod = async () => {
    setBusy("mod");
    try {
      const v = await api.updateMod();
      toast.success(`Мод обновлён до ${v} — установится при следующем запуске`);
      await checkMod();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Section title="Обновления" hint="Лаунчер проверяет и ставит обновления сам">
      <div className="versions">
        <div>
          <span className="muted small">Лаунчер</span>
          <strong>{updater?.currentVersion ?? "—"}</strong>
          <span className={`small ${updater?.stage === "error" ? "danger" : "muted"}`}>{launcherStatusText(updater)}</span>
          {updater?.stage === "downloading" && updater.progress != null && (
            <div className="update-progress">
              <span style={{ width: `${Math.round(updater.progress * 100)}%` }} />
            </div>
          )}
        </div>
        <div>
          <span className="muted small">Elysium Visuals</span>
          <strong>{mod?.modVersion ?? "—"}</strong>
          {mod?.modError && <span className="small danger">{mod.modError}</span>}
          {mod?.modUpdate && (
            <button className="btn btn-primary btn-sm" disabled={!!busy} onClick={updateMod}>
              <ArrowDownToLine size={14} /> Обновить до {mod.modUpdate.version}
            </button>
          )}
        </div>
      </div>
      <div className="row">
        <button className="btn btn-ghost" disabled={!!busy || launcherBusy} onClick={check}>
          {busy === "check" || launcherBusy ? <RefreshCw size={16} className="spin" /> : <RotateCcw size={16} />} Проверить
          обновления
        </button>
      </div>
    </Section>
  );
}
