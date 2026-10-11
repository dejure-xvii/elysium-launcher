import { motion } from "framer-motion";
import { CircleUserRound, Play, Puzzle, Settings, SquareTerminal, Users, type LucideIcon } from "lucide-react";
import logo from "../assets/logo.svg";
import { useElysium } from "../store/elysium";
import { useLaunch } from "../store/launch";
import { ElysiumAvatar } from "./ElysiumAccountDialog";
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
  const { user, openDialog } = useElysium();

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
      <button
        className="sidebar-item sidebar-account"
        onClick={openDialog}
        aria-label={user ? `Аккаунт Elysium: ${user.username}` : "Войти в аккаунт Elysium"}
      >
        {user ? <ElysiumAvatar user={user} size={30} /> : <CircleUserRound size={22} strokeWidth={1.9} />}
        <span className="sidebar-tooltip">{user ? user.username : "Аккаунт Elysium"}</span>
      </button>
    </nav>
  );
}
