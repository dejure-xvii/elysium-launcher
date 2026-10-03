import { MemoryStick } from "lucide-react";
import logo from "../assets/logo.svg";
import { AccountPicker } from "../components/AccountPicker";
import { PlayButton } from "../components/PlayButton";
import { useSelectedAccount } from "../store/accounts";
import { useNav } from "../store/nav";
import { formatGb, useSettings } from "../store/settings";

export function PlayPage() {
  const settings = useSettings((s) => s.settings);
  const account = useSelectedAccount();
  const go = useNav((s) => s.go);

  return (
    <div className="play-page">
      <div className="hero">
        <img className="hero-logo" src={logo} alt="" draggable={false} />
        <h1 className="hero-title">
          Elysium <span>Visuals</span>
        </h1>
        <p className="hero-sub">Minecraft 26.2 · Fabric</p>
      </div>

      <PlayButton disabled={!account} />

      <div className="play-chips">
        <AccountPicker />
        <button className="chip glass glass-interactive" onClick={() => go("settings")}>
          <MemoryStick size={18} className="accent" />
          <span className="chip-text">
            <span className="chip-title">{settings ? formatGb(settings.memoryMb) : "—"}</span>
            <span className="chip-sub">Выделено памяти</span>
          </span>
        </button>
      </div>
    </div>
  );
}
