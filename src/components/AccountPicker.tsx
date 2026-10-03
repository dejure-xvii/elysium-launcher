import { AnimatePresence, motion } from "framer-motion";
import { Check, ChevronDown, UserPlus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useAccounts, useSelectedAccount } from "../store/accounts";
import { useNav } from "../store/nav";
import { SkinHead } from "./SkinHead";

export function AccountPicker() {
  const accounts = useAccounts((s) => s.accounts);
  const select = useAccounts((s) => s.select);
  const current = useSelectedAccount();
  const go = useNav((s) => s.go);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  if (!current) {
    return (
      <button className="chip glass glass-interactive" onClick={() => go("accounts")}>
        <UserPlus size={18} />
        <span className="chip-text">
          <span className="chip-title">Добавить аккаунт</span>
          <span className="chip-sub">Microsoft или офлайн</span>
        </span>
      </button>
    );
  }

  return (
    <div className="picker" ref={ref}>
      <button className="chip glass glass-interactive" onClick={() => setOpen(!open)}>
        <SkinHead skinUrl={current.skinUrl} size={30} />
        <span className="chip-text">
          <span className="chip-title">{current.username}</span>
          <span className="chip-sub">{current.kind === "microsoft" ? "Microsoft" : "Офлайн"}</span>
        </span>
        <ChevronDown size={16} className={`chev ${open ? "up" : ""}`} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            className="picker-menu glass"
            initial={{ opacity: 0, y: 6, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.98 }}
            transition={{ duration: 0.15 }}
          >
            {accounts.map((a) => (
              <button
                key={a.id}
                className="picker-item"
                onClick={() => {
                  select(a.id);
                  setOpen(false);
                }}
              >
                <SkinHead skinUrl={a.skinUrl} size={24} />
                <span className="picker-name">{a.username}</span>
                <span className="picker-kind">{a.kind === "microsoft" ? "MS" : "офлайн"}</span>
                {a.id === current.id && <Check size={15} className="accent" />}
              </button>
            ))}
            <div className="picker-sep" />
            <button
              className="picker-item"
              onClick={() => {
                setOpen(false);
                go("accounts");
              }}
            >
              <UserPlus size={18} />
              <span className="picker-name">Управление аккаунтами</span>
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
