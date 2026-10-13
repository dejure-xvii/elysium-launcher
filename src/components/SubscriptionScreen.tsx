import { motion } from "framer-motion";
import { ExternalLink, KeyRound, LoaderCircle, LogOut, RefreshCw, ShoppingCart } from "lucide-react";
import { useState } from "react";
import logo from "../assets/logo.svg";
import { errorText } from "../api";
import { dateText, useElysium } from "../store/elysium";
import { toast } from "../store/toasts";
import { openElysiumPage } from "./LoginScreen";

/** Full-window screen for an Elysium account without an active subscription. */
export function SubscriptionScreen() {
  const { status, load, logout } = useElysium();
  const [checking, setChecking] = useState(false);
  if (status?.state !== "inactive") return null;
  const { user, subscription, offline } = status;

  const recheck = async () => {
    setChecking(true);
    await load();
    setChecking(false);
    const now = useElysium.getState().status;
    if (now?.state === "online") toast.success("Подписка активна — приятной игры!");
    else if (now?.state === "inactive") toast.error(now.offline ? "Нет связи с сервером Elysium" : "Подписка всё ещё не активна");
  };

  const signOut = () => logout().catch((e) => toast.error(errorText(e)));

  return (
    <div className="login-screen">
      <motion.div
        className="login-card glass elysium-form"
        initial={{ opacity: 0, y: 12, filter: "blur(4px)" }}
        animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
        transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      >
        <img className="login-logo" src={logo} alt="" draggable={false} />
        <div className="login-head">
          <h2>Подписка не активна</h2>
          <p className="muted small">
            {subscription.expiresAt
              ? `Подписка аккаунта ${user.username} закончилась ${dateText(subscription.expiresAt)}.`
              : `У аккаунта ${user.username} пока нет подписки.`}{" "}
            Купите подписку на сайте и активируйте ключ в личном кабинете — затем нажмите «Проверить снова».
          </p>
          {offline && (
            <p className="small warn-text">Нет связи с сервером Elysium: статус взят из последней проверки.</p>
          )}
        </div>
        <button type="button" className="btn btn-primary" onClick={() => openElysiumPage("buy")}>
          <ShoppingCart size={16} /> Купить <ExternalLink size={14} />
        </button>
        <button type="button" className="btn btn-ghost" onClick={() => openElysiumPage("account")}>
          <KeyRound size={16} /> Активировать ключ <ExternalLink size={14} />
        </button>
        <button type="button" className="btn btn-ghost" onClick={recheck} disabled={checking}>
          {checking ? <LoaderCircle size={16} className="spin" /> : <RefreshCw size={16} />} Проверить снова
        </button>
        <button type="button" className="link login-forgot small" onClick={signOut}>
          <LogOut size={12} /> Выйти из аккаунта {user.username}
        </button>
      </motion.div>
    </div>
  );
}
