import { motion } from "framer-motion";
import { Play, Puzzle, Settings, SquareTerminal, Users, type LucideIcon } from "lucide-react";
import logo from "../assets/logo.svg";
import { useLaunch } from "../store/launch";
import { useNav, type Page } from "../store/nav";

const items: { page: Page; icon: LucideIcon; label: string }[] = [
  { page: "play", icon: Play, label: "Играть" },
  { page: "mods", icon: Puzzle, label: "Моды" },
  { page: "settings", icon: Settings, label: "Настройки" },
  { page: "console", icon: SquareTerminal, label: "Консоль" },
  { page: "accounts", icon: Users, label: "Аккаунты" },
];

export function Sidebar() {
  const { page, go } = useNav();
  const running = useLaunch((s) => s.phase === "running");

  return (
    <nav className="sidebar glass">
      <img className="sidebar-logo" src={logo} alt="" draggable={false} />
      <div className="sidebar-items">
        {items.map(({ page: p, icon: Icon, label }) => (
          <button
            key={p}
            className={`sidebar-item ${page === p ? "active" : ""}`}
            onClick={() => go(p)}
            aria-label={label}
          >
            {page === p && (
              <motion.span
                layoutId="sidebar-active"
                className="sidebar-active"
                transition={{ type: "spring", stiffness: 500, damping: 38 }}
              />
            )}
            <Icon size={20} strokeWidth={page === p ? 2.3 : 1.9} />
            {p === "console" && running && <span className="sidebar-dot" />}
            <span className="sidebar-tooltip">{label}</span>
          </button>
        ))}
      </div>
    </nav>
  );
}
