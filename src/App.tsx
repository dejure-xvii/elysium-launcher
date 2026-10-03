import { AnimatePresence, motion } from "framer-motion";
import { useEffect } from "react";
import { api, errorText, events } from "./api";
import { AnimatedBackground } from "./components/AnimatedBackground";
import { CrashDialog } from "./components/CrashDialog";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { AccountsPage } from "./pages/AccountsPage";
import { ConsolePage } from "./pages/ConsolePage";
import { PlayPage } from "./pages/PlayPage";
import { SettingsPage } from "./pages/SettingsPage";
import { useAccounts } from "./store/accounts";
import { useConsole } from "./store/console";
import { useLaunch } from "./store/launch";
import { useNav } from "./store/nav";
import { useSettings } from "./store/settings";
import { toast } from "./store/toasts";

const pages = {
  play: PlayPage,
  settings: SettingsPage,
  console: ConsolePage,
  accounts: AccountsPage,
};

export default function App() {
  const page = useNav((s) => s.page);
  const Page = pages[page];

  useEffect(() => {
    Promise.all([useSettings.getState().load(), useAccounts.getState().load()]).catch((e) =>
      toast.error(errorText(e)),
    );
    // A game may still be running if the webview was reloaded.
    api.isBusy().then((busy) => busy && useLaunch.setState({ phase: "running" }));

    const subs = [
      events.onProgress((p) => useLaunch.getState().onProgress(p)),
      events.onLog((l) => useConsole.getState().push(l)),
      events.onExit((x) => useLaunch.getState().onExit(x)),
      events.onAccountsChanged(() => useAccounts.getState().load()),
    ];
    const block = (e: MouseEvent) => e.preventDefault();
    window.addEventListener("contextmenu", block);
    return () => {
      subs.forEach((s) => s.then((un) => un()));
      window.removeEventListener("contextmenu", block);
    };
  }, []);

  return (
    <div className="app">
      <AnimatedBackground />
      <TitleBar />
      <div className="layout">
        <Sidebar />
        <main className="content">
          <AnimatePresence mode="wait">
            <motion.div
              key={page}
              className="page-host"
              initial={{ opacity: 0, y: 10, filter: "blur(4px)" }}
              animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
              exit={{ opacity: 0, y: -6, filter: "blur(4px)" }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            >
              <Page />
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
      <Toasts />
      <CrashDialog />
    </div>
  );
}
