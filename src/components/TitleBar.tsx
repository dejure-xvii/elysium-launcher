import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "lucide-react";

export function TitleBar() {
  const win = getCurrentWindow();
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-title" data-tauri-drag-region>
        Elysium Launcher
      </div>
      <div className="titlebar-buttons">
        <button className="titlebar-btn" aria-label="Свернуть" onClick={() => win.minimize()}>
          <Minus size={15} />
        </button>
        <button className="titlebar-btn close" aria-label="Закрыть" onClick={() => win.close()}>
          <X size={15} />
        </button>
      </div>
    </header>
  );
}
