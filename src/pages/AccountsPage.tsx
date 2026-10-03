import { Check, Copy, ExternalLink, LoaderCircle, LogIn, Trash, UserPlus } from "lucide-react";
import { AnimatePresence, motion } from "framer-motion";
import { useState } from "react";
import { api, errorText, type DeviceCode } from "../api";
import { Modal } from "../components/Modal";
import { SkinHead } from "../components/SkinHead";
import { NICK_RE, useAccounts, useSelectedAccount } from "../store/accounts";
import { useSettings } from "../store/settings";
import { toast } from "../store/toasts";

export function AccountsPage() {
  const { accounts, select, remove, addOffline, added } = useAccounts();
  const current = useSelectedAccount();
  const msAvailable = useSettings((s) => s.system?.msLoginAvailable ?? false);
  const [nick, setNick] = useState("");
  const [code, setCode] = useState<DeviceCode | null>(null);

  const nickValid = NICK_RE.test(nick);

  const submitOffline = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nickValid) return;
    try {
      const a = await addOffline(nick);
      setNick("");
      toast.success(`Аккаунт ${a.username} добавлен`);
    } catch (err) {
      toast.error(errorText(err));
    }
  };

  const loginMicrosoft = async () => {
    try {
      const c = await api.msLoginStart();
      setCode(c);
      const account = await api.msLoginFinish();
      await added(account);
      toast.success(`Вход выполнен: ${account.username}`);
    } catch (err) {
      const msg = errorText(err);
      if (msg !== "Вход отменён") toast.error(msg);
    } finally {
      setCode(null);
    }
  };

  const cancelLogin = () => {
    api.msLoginCancel();
    setCode(null);
  };

  return (
    <div className="page">
      <h2 className="page-title">Аккаунты</h2>

      <section className="section glass">
        <div className="section-head">
          <h3>Сохранённые</h3>
          <span className="muted small">{accounts.length || "Нет аккаунтов"}</span>
        </div>
        <div className="accounts">
          <AnimatePresence initial={false}>
            {accounts.map((a) => {
              const active = a.id === current?.id;
              return (
                <motion.div
                  key={a.id}
                  layout
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className={`account ${active ? "active" : ""}`}
                  onClick={() => select(a.id)}
                >
                  <SkinHead skinUrl={a.skinUrl} size={38} />
                  <div className="account-info">
                    <strong>{a.username}</strong>
                    <span className={`badge ${a.kind === "microsoft" ? "badge-accent" : ""}`}>
                      {a.kind === "microsoft" ? "Microsoft" : "Офлайн"}
                    </span>
                  </div>
                  {active && (
                    <span className="account-check">
                      <Check size={16} />
                    </span>
                  )}
                  <button
                    className="icon-btn danger"
                    aria-label="Удалить"
                    onClick={(e) => {
                      e.stopPropagation();
                      remove(a.id).catch((err) => toast.error(errorText(err)));
                    }}
                  >
                    <Trash size={16} />
                  </button>
                </motion.div>
              );
            })}
          </AnimatePresence>
        </div>
      </section>

      <div className="two-col">
        <section className="section glass">
          <div className="section-head">
            <h3>Microsoft</h3>
            <span className="muted small">Лицензия, все серверы</span>
          </div>
          <p className="muted small">
            Официальный вход через Microsoft → Xbox Live → Minecraft. Пароль вводится только на сайте Microsoft, лаунчер
            хранит токен в защищённом хранилище Windows.
          </p>
          <button className="btn btn-primary" disabled={!msAvailable} onClick={loginMicrosoft}>
            <LogIn size={16} /> Войти через Microsoft
          </button>
          {!msAvailable && <span className="muted small">Не настроен Client ID приложения (см. README)</span>}
        </section>

        <section className="section glass">
          <div className="section-head">
            <h3>Офлайн</h3>
            <span className="muted small">Одиночная игра и серверы без авторизации</span>
          </div>
          <form className="row" onSubmit={submitOffline}>
            <input
              className={`input grow ${nick && !nickValid ? "invalid" : ""}`}
              placeholder="Никнейм"
              value={nick}
              maxLength={16}
              onChange={(e) => setNick(e.target.value)}
              spellCheck={false}
            />
            <button className="btn btn-primary" type="submit" disabled={!nickValid}>
              <UserPlus size={16} /> Добавить
            </button>
          </form>
          <span className={`small ${nick && !nickValid ? "danger" : "muted"}`}>
            3–16 символов: латиница, цифры и _
          </span>
        </section>
      </div>

      <Modal open={!!code} onClose={cancelLogin}>
        {code && <DeviceCodeView code={code} onCancel={cancelLogin} />}
      </Modal>
    </div>
  );
}

function DeviceCodeView({ code, onCancel }: { code: DeviceCode; onCancel: () => void }) {
  const [copied, setCopied] = useState(false);

  const copyAndOpen = async () => {
    try {
      await navigator.clipboard.writeText(code.userCode);
      setCopied(true);
    } catch {
      /* clipboard may be unavailable; the code is visible anyway */
    }
    api.openUrl(code.verificationUri).catch((e) => toast.error(errorText(e)));
  };

  return (
    <>
      <h2>Вход через Microsoft</h2>
      <p className="muted">
        Откройте страницу Microsoft и введите этот код. Окно закроется само, когда вход будет подтверждён.
      </p>
      <div className="device-code">{code.userCode}</div>
      <div className="modal-actions">
        <button className="btn btn-primary" onClick={copyAndOpen}>
          {copied ? <Check size={16} /> : <Copy size={16} />} Скопировать и открыть <ExternalLink size={14} />
        </button>
        <button className="btn btn-ghost" onClick={onCancel}>
          Отмена
        </button>
      </div>
      <div className="waiting muted small">
        <LoaderCircle size={14} className="spin" /> Ожидание подтверждения…
      </div>
    </>
  );
}
