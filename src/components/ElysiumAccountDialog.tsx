import { Eye, EyeOff, ExternalLink, LoaderCircle, LogIn, LogOut, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { api, errorText, type ElysiumPage, type ElysiumUser } from "../api";
import { useElysium } from "../store/elysium";
import { toast } from "../store/toasts";
import { Modal } from "./Modal";

const openPage = (page: ElysiumPage) => api.elysiumOpenPage(page).catch((e) => toast.error(errorText(e)));

/** Sign-in to the Elysium account, or its profile when signed in. */
export function ElysiumAccountDialog() {
  const { user, dialogOpen, closeDialog } = useElysium();
  return (
    <Modal open={dialogOpen} onClose={closeDialog}>
      {user ? <Profile user={user} /> : <LoginForm />}
    </Modal>
  );
}

export function ElysiumAvatar({ user, size }: { user: ElysiumUser | null; size: number }) {
  return (
    <span className="elysium-avatar" style={{ width: size, height: size, fontSize: size * 0.44 }}>
      {user?.username[0]}
    </span>
  );
}

function LoginForm() {
  const { login, closeDialog } = useElysium();
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cyrillic = /[Ѐ-ӿ]/.test(password);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !password) return setError("Введите логин или почту и пароль");
    setBusy(true);
    setError(null);
    try {
      const u = await login(name, password);
      setPassword("");
      closeDialog();
      toast.success(`Вы вошли в аккаунт Elysium: ${u.username}`);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="elysium-form" onSubmit={submit}>
      <div>
        <h2>Аккаунт Elysium</h2>
        <p className="muted">
          Необязателен для игры. Пригодится для синхронизации настроек и друзей. Это не Minecraft-аккаунт.
        </p>
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
      <div className="modal-actions">
        <button className="btn btn-primary grow" type="submit" disabled={busy}>
          {busy ? <LoaderCircle size={16} className="spin" /> : <LogIn size={16} />} Войти
        </button>
      </div>
      <div className="elysium-links small">
        <button type="button" className="link" onClick={() => openPage("register")}>
          Регистрация <ExternalLink size={12} />
        </button>
        <button type="button" className="link" onClick={() => openPage("forgot")}>
          Забыли пароль? <ExternalLink size={12} />
        </button>
      </div>
    </form>
  );
}

function Profile({ user }: { user: ElysiumUser }) {
  const { logout, closeDialog } = useElysium();
  const [busy, setBusy] = useState(false);

  const signOut = async () => {
    setBusy(true);
    try {
      await logout();
      closeDialog();
      toast.success("Вы вышли из аккаунта Elysium");
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="elysium-profile">
        <ElysiumAvatar user={user} size={52} />
        <div className="elysium-profile-info">
          <h2>{user.username}</h2>
          <span className="muted small">{user.email}</span>
        </div>
      </div>
      {!user.emailVerified && (
        <span className="small warn-text">
          <TriangleAlert size={13} /> Почта не подтверждена — письмо можно отправить ещё раз в личном кабинете
        </span>
      )}
      <p className="muted">
        Аккаунт Elysium не влияет на запуск игры и не связан с Minecraft-аккаунтами. Скоро здесь появятся синхронизация
        настроек и друзья.
      </p>
      <div className="modal-actions">
        <button className="btn btn-primary" onClick={() => openPage("account")}>
          Личный кабинет <ExternalLink size={14} />
        </button>
        <button className="btn btn-ghost" onClick={signOut} disabled={busy}>
          {busy ? <LoaderCircle size={16} className="spin" /> : <LogOut size={16} />} Выйти
        </button>
      </div>
    </>
  );
}
