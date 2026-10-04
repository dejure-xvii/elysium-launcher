import { FolderOpen } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, errorText, type BuiltinMod, type ModsView } from "../api";
import { useLaunch } from "../store/launch";
import { useSettings } from "../store/settings";
import { toast } from "../store/toasts";

function status(m: BuiltinMod): string | null {
  if (m.replacedByUser) return "используется ваша копия из папки модов";
  if (!m.enabled) return m.installed ? "будет удалён при следующем запуске" : null;
  if (!m.installed) return "будет установлен при следующем запуске";
  return null;
}

function BuiltinRow({ mod, locked, onToggle }: { mod: BuiltinMod; locked: boolean; onToggle: (on: boolean) => void }) {
  const note = status(mod);
  const dependency = mod.dependencyOf.length > 0 && mod.required;
  return (
    <div className="mod-row">
      <div className="mod-info">
        <div className="mod-name">
          {mod.name}
          {mod.version && <span className="muted small">{mod.version}</span>}
        </div>
        <div className="mod-meta">
          <span className="badge badge-accent">встроенный</span>
          {dependency ? (
            <span className="muted small">нужен для {mod.dependencyOf.join(", ")}</span>
          ) : (
            mod.required && <span className="badge">обязательный</span>
          )}
          {note && <span className="muted small">{note}</span>}
        </div>
      </div>
      <label className={`toggle ${mod.required || locked ? "toggle-disabled" : ""}`} title={mod.required ? "Обязательный мод нельзя выключить" : undefined}>
        <input
          type="checkbox"
          checked={mod.enabled}
          disabled={mod.required || locked}
          onChange={(e) => onToggle(e.target.checked)}
        />
        <span className="toggle-track">
          <span className="toggle-thumb" />
        </span>
      </label>
    </div>
  );
}

export function ModsPage() {
  const [view, setView] = useState<ModsView | null>(null);
  const phase = useLaunch((s) => s.phase);
  const locked = phase !== "idle";

  const reload = useCallback(() => {
    api.listMods().then(setView, (e) => toast.error(errorText(e)));
  }, []);

  // Refresh after installs/launches change the mods folder.
  useEffect(reload, [reload, phase]);

  const toggle = async (id: string, on: boolean) => {
    try {
      const settings = await api.setModEnabled(id, on);
      useSettings.setState({ settings });
      reload();
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <div className="page">
      <h2 className="page-title">Моды</h2>

      <section className="section glass">
        <div className="section-head">
          <h3>Встроенные</h3>
          <span className="muted small">Ставятся с Modrinth, изменения применяются при следующем запуске</span>
        </div>
        <div className="mod-list">
          {view?.builtin.map((m) => (
            <BuiltinRow key={m.id} mod={m} locked={locked} onToggle={(on) => toggle(m.id, on)} />
          ))}
        </div>
      </section>

      <section className="section glass">
        <div className="section-head">
          <h3>Ваши моды</h3>
          <span className="muted small">Лаунчер их не изменяет и не удаляет</span>
        </div>
        {view && view.user.length > 0 ? (
          <div className="mod-list">
            {view.user.map((m) => (
              <div className="mod-row" key={m.file}>
                <div className="mod-info">
                  <div className="mod-name">
                    {m.name}
                    {m.version && <span className="muted small">{m.version}</span>}
                  </div>
                  <div className="mod-meta">
                    <span className="muted small">{m.file}</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <span className="muted small">Положите .jar в папку mods, и мод появится здесь.</span>
        )}
        <div className="row">
          <button className="btn btn-ghost" onClick={() => api.openModsDir().catch((e) => toast.error(errorText(e)))}>
            <FolderOpen size={16} /> Открыть папку модов
          </button>
        </div>
      </section>
    </div>
  );
}
