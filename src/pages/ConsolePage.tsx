import { Copy, FolderOpen, Trash } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import { useConsole } from "../store/console";
import { useLaunch } from "../store/launch";
import { toast } from "../store/toasts";

export function ConsolePage() {
  const { lines, clear, text } = useConsole();
  const phase = useLaunch((s) => s.phase);
  const ref = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);

  // Stick to the bottom while following.
  useLayoutEffect(() => {
    if (follow && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines, follow]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text());
      toast.success("Лог скопирован");
    } catch (e) {
      toast.error(errorText(e));
    }
  };

  return (
    <div className="page console-page">
      <div className="console-head">
        <h2 className="page-title">Консоль</h2>
        <span className={`badge ${phase === "running" ? "badge-ok" : ""}`}>
          {phase === "running" ? "Игра запущена" : phase === "preparing" ? "Подготовка" : "Не запущена"}
        </span>
        <span className="muted small">{lines.length} строк</span>
        <div className="grow" />
        <button className="btn btn-ghost btn-sm" onClick={copy} disabled={!lines.length}>
          <Copy size={14} /> Скопировать лог
        </button>
        <button className="btn btn-ghost btn-sm" onClick={() => api.openLogsDir().catch((e) => toast.error(errorText(e)))}>
          <FolderOpen size={14} /> Папка логов
        </button>
        <button className="btn btn-ghost btn-sm" onClick={clear} disabled={!lines.length} aria-label="Очистить">
          <Trash size={14} />
        </button>
      </div>

      <div className="console glass" ref={ref}>
        {lines.length === 0 ? (
          <div className="console-empty muted">Здесь появятся логи игры после запуска</div>
        ) : (
          lines.map((l) => (
            <div key={l.id} className={`log log-${l.level}`}>
              {l.text}
            </div>
          ))
        )}
      </div>
      {!follow && lines.length > 0 && (
        <button className="btn btn-primary btn-sm console-follow" onClick={() => setFollow(true)}>
          К последним строкам
        </button>
      )}
    </div>
  );
}
