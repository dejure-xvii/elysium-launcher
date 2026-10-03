import { formatGb } from "../store/settings";

const MIN = 2048;
const STEP = 512;

interface Props {
  value: number;
  max: number;
  recommended: number;
  total: number;
  onChange: (mb: number) => void;
}

export function RamSlider({ value, max, recommended, total, onChange }: Props) {
  const span = Math.max(max - MIN, STEP);
  const pct = ((value - MIN) / span) * 100;
  const recPct = ((recommended - MIN) / span) * 100;
  const isRec = value === recommended;

  return (
    <div className="ram">
      <div className="ram-head">
        <span className="ram-value">{formatGb(value)}</span>
        <span className="ram-total">из {formatGb(total)}</span>
        {isRec ? (
          <span className="badge badge-accent">Рекомендуется</span>
        ) : (
          <button className="btn btn-ghost btn-sm" onClick={() => onChange(recommended)}>
            Рекомендуемое: {formatGb(recommended)}
          </button>
        )}
      </div>
      <div className="ram-track-wrap">
        <input
          type="range"
          className="range"
          min={MIN}
          max={max}
          step={STEP}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          style={{ "--pct": `${pct}%` } as React.CSSProperties}
        />
        <div className={`ram-rec ${isRec ? "on" : ""}`} style={{ left: `${recPct}%` }} title="Рекомендуемое значение" />
      </div>
      <div className="ram-scale">
        <span>{formatGb(MIN)}</span>
        <span>{formatGb(max)}</span>
      </div>
    </div>
  );
}
