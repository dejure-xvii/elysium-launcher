import { AnimatePresence, motion } from "framer-motion";
import { LoaderCircle, Play, Square } from "lucide-react";
import { useLaunch } from "../store/launch";

export function PlayButton({ disabled }: { disabled?: boolean }) {
  const { phase, progress, launch, stop } = useLaunch();
  const pct = progress && progress.total > 0 ? Math.min(100, (progress.current / progress.total) * 100) : null;

  return (
    <div className="play-wrap">
      <AnimatePresence mode="wait" initial={false}>
        {phase === "idle" && (
          <motion.button
            key="play"
            className="play-btn"
            disabled={disabled}
            onClick={launch}
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: disabled ? 0.5 : 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.96 }}
            whileHover={disabled ? undefined : { scale: 1.03 }}
            whileTap={disabled ? undefined : { scale: 0.97 }}
            transition={{ duration: 0.18 }}
          >
            <Play size={22} fill="currentColor" />
            Играть
          </motion.button>
        )}

        {phase === "preparing" && (
          <motion.div
            key="progress"
            className="play-progress glass"
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.96 }}
            transition={{ duration: 0.18 }}
          >
            <div className="play-progress-row">
              <LoaderCircle size={16} className="spin" />
              <span className="play-progress-label">{progress?.label ?? "Подготовка…"}</span>
              {pct !== null && <span className="play-progress-pct">{Math.floor(pct)}%</span>}
            </div>
            <div className="bar">
              <div
                className={`bar-fill ${pct === null ? "indeterminate" : ""}`}
                style={pct === null ? undefined : { width: `${pct}%` }}
              />
            </div>
          </motion.div>
        )}

        {phase === "running" && (
          <motion.div
            key="running"
            className="play-running glass"
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.96 }}
            transition={{ duration: 0.18 }}
          >
            <span className="pulse-dot" />
            <span>Игра запущена</span>
            <button className="btn btn-ghost btn-sm" onClick={stop}>
              <Square size={13} fill="currentColor" /> Остановить
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
