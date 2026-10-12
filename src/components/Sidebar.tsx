import { AnimatePresence, motion } from "framer-motion";
import { ExternalLink, LoaderCircle, LogOut, Play, Puzzle, Settings, SquareTerminal, Users, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import logo from "../assets/logo.svg";
import { errorText } from "../api";
import { useElysium, useElysiumUser } from "../store/elysium";
import { useLaunch } from "../store/launch";
import { useNav, type Page } from "../store/nav";
import { toast } from "../store/toasts";
import { ElysiumAvatar, openElysiumPage } from "./LoginScreen";

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
      <AccountMenu />
    </nav>
  );
}

/** Signed-in Elysium account: avatar with a menu (profile on the site, sign out). */
function AccountMenu() {
  const user = useElysiumUser();
  const logout = useElysium((s) => s.logout);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  if (!user) return null;

  const signOut = async () => {
    setBusy(true);
    try {
      await logout();
      toast.success("Вы вышли из аккаунта Elysium");
    } catch (err) {
      toast.error(errorText(err));
      setBusy(false);
    }
  };

  return (
    <div className="sidebar-account" ref={ref}>
      <button
        className="sidebar-item"
        onClick={() => setOpen(!open)}
        aria-label={`Аккаунт Elysium: ${user.username}`}
        aria-expanded={open}
      >
        <ElysiumAvatar user={user} size={30} />
        {!open && <span className="sidebar-tooltip">{user.username}</span>}
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            className="account-menu glass"
            initial={{ opacity: 0, x: -6 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -6 }}
            transition={{ duration: 0.14 }}
          >
            <div className="account-menu-head">
              <ElysiumAvatar user={user} size={36} />
              <div className="account-menu-name">
                <strong>{user.username}</strong>
                <span className="muted small">Аккаунт Elysium</span>
              </div>
            </div>
            <button className="account-menu-item" onClick={() => (setOpen(false), openElysiumPage("account"))}>
              <ExternalLink size={15} /> Профиль на сайте
            </button>
            <button className="account-menu-item" onClick={signOut} disabled={busy}>
              {busy ? <LoaderCircle size={15} className="spin" /> : <LogOut size={15} />} Выйти
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
