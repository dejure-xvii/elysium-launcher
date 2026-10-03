import { FileText, SquareTerminal, TriangleAlert } from "lucide-react";
import { api, errorText } from "../api";
import { useLaunch } from "../store/launch";
import { useNav } from "../store/nav";
import { toast } from "../store/toasts";
import { Modal } from "./Modal";

export function CrashDialog() {
  const crash = useLaunch((s) => s.crash);
  const dismiss = useLaunch((s) => s.dismissCrash);
  const go = useNav((s) => s.go);

  const open = (path: string | null) => {
    if (!path) return;
    api.openFile(path).catch((e) => toast.error(errorText(e)));
  };

  return (
    <Modal open={!!crash} onClose={dismiss}>
      <div className="modal-icon danger">
        <TriangleAlert size={26} />
      </div>
      <h2>Игра завершилась с ошибкой</h2>
      <p className="muted">
        {crash?.code != null ? `Код выхода ${crash.code}. ` : ""}
        Чаще всего причина — нехватка памяти, несовместимый мод или драйвер видеокарты. Подробности — в логе.
      </p>
      <div className="modal-actions">
        {crash?.crashReport && (
          <button className="btn btn-primary" onClick={() => open(crash.crashReport)}>
            <FileText size={16} /> Отчёт о краше
          </button>
        )}
        {crash?.logFile && (
          <button className={`btn ${crash.crashReport ? "btn-ghost" : "btn-primary"}`} onClick={() => open(crash.logFile)}>
            <FileText size={16} /> Открыть лог
          </button>
        )}
        <button
          className="btn btn-ghost"
          onClick={() => {
            dismiss();
            go("console");
          }}
        >
          <SquareTerminal size={16} /> Консоль
        </button>
        <button className="btn btn-ghost" onClick={dismiss}>
          Закрыть
        </button>
      </div>
    </Modal>
  );
}
