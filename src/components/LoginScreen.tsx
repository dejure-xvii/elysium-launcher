import { motion } from "framer-motion";
import { Eye, EyeOff, ExternalLink, LoaderCircle, LogIn, TriangleAlert, UserPlus } from "lucide-react";
import { useState } from "react";
import logo from "../assets/logo.svg";
import { api, errorText, type ElysiumPage, type ElysiumUser } from "../api";
import { useElysium } from "../store/elysium";
import { toast } from "../store/toasts";

export const openElysiumPage = (page: ElysiumPage) =>
  api.elysiumOpenPage(page).catch((e) => toast.error(errorText(e)));

export function ElysiumAvatar({ user, size }: { user: ElysiumUser | null; size: number }) {
  return (
    <span className="elysium-avatar" style={{ width: size, height: size, fontSize: size * 0.44 }}>
      {user?.username[0]}
    </span>
  );
}

/** Full-window sign-in: the Elysium account is required to use the launcher. */
export function LoginScreen() {
  const { login, status } = useElysium();
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(
    status?.state === "unreachable"
      ? "Нет связи с сервером аккаунтов, а последняя проверка входа была больше 7 дней назад. Проверьте интернет и войдите снова."
      : null,
  );
  const cyrillic = /[Ѐ-ӿ]/.test(password);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !password) return setError("Введите логин или почту и пароль");
    setBusy(true);
    setError(null);
    try {
      const u = await login(name, password);
      setPassword("");
      toast.success(`Вы вошли в аккаунт Elysium: ${u.username}`);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="login-screen">
      <motion.form
        className="login-card glass elysium-form"
        onSubmit={submit}
        initial={{ opacity: 0, y: 12, filter: "blur(4px)" }}
        animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
        transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      >
        <img className="login-logo" src={logo} alt="" draggable={false} />
        <div className="login-head">
          <h2>Войдите в аккаунт Elysium</h2>
          <p className="muted small">Это не Minecraft-аккаунт: его выбирают после входа, на вкладке «Аккаунты».</p>
        </div>
        <label className="field">
          Логин или почта
          <input
            className="input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoComplete="username"
            spellCheck={false}
            maxLength={254}
            autoFocus
          />
        </label>
        <label className="field">
          Пароль
          <span className="input-wrap">
            <input
              className="input"
              type={show ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
              maxLength={128}
            />
            <button
              type="button"
              className="icon-btn input-eye"
              aria-label={show ? "Скрыть пароль" : "Показать пароль"}
              onClick={() => setShow(!show)}
            >
              {show ? <EyeOff size={16} /> : <Eye size={16} />}
            </button>
          </span>
        </label>
        {cyrillic && (
          <span className="small warn-text">
            <TriangleAlert size={13} /> Включена русская раскладка — в пароле только английские буквы
          </span>
        )}
        {error && <div className="form-error">{error}</div>}
        <button className="btn btn-primary" type="submit" disabled={busy}>
          {busy ? <LoaderCircle size={16} className="spin" /> : <LogIn size={16} />} Войти
        </button>
        <button type="button" className="link login-forgot small" onClick={() => openElysiumPage("forgot")}>
          Забыли пароль? <ExternalLink size={12} />
        </button>
        <div className="login-register">
          <p className="muted small">
            Нет аккаунта? Зарегистрируйтесь на сайте — это бесплатно и займёт минуту. После регистрации вернитесь сюда
            и войдите.
          </p>
          <button type="button" className="btn btn-ghost" onClick={() => openElysiumPage("register")}>
            <UserPlus size={18} /> Зарегистрироваться <ExternalLink size={14} />
          </button>
        </div>
      </motion.form>
    </div>
  );
}
