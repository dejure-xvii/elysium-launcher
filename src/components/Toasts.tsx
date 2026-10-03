import { AnimatePresence, motion } from "framer-motion";
import { CircleCheck, Info, TriangleAlert, X } from "lucide-react";
import { useToasts } from "../store/toasts";

const icons = { info: Info, success: CircleCheck, error: TriangleAlert };

export function Toasts() {
  const { toasts, dismiss } = useToasts();
  return (
    <div className="toasts">
      <AnimatePresence>
        {toasts.map((t) => {
          const Icon = icons[t.kind];
          return (
            <motion.div
              key={t.id}
              layout
              className={`toast glass toast-${t.kind}`}
              initial={{ opacity: 0, x: 40 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 40 }}
              transition={{ duration: 0.2 }}
            >
              <Icon size={18} className="toast-icon" />
              <span className="toast-text">{t.text}</span>
              <button className="icon-btn" onClick={() => dismiss(t.id)} aria-label="Закрыть">
                <X size={14} />
              </button>
            </motion.div>
          );
        })}
      </AnimatePresence>
    </div>
  );
}
