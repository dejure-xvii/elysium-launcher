import { useState } from "react";

// Steve's face, 8×8, used for offline accounts and as a fallback.
const P: Record<string, string> = {
  H: "#2f200d", h: "#3f2a12", S: "#b5866a", s: "#a37157", W: "#f4f4f4",
  E: "#4b3a9b", N: "#7a4a33", M: "#5e3426",
};
const STEVE = [
  "HHHHHHHH",
  "HHHhhHHH",
  "HSSsSSSH",
  "SSSSSSSS",
  "SWESSEWS",
  "SSSNNSSS",
  "SSMSSMSS",
  "SSMMMMSS",
];

function SteveFace({ size }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 8 8" shapeRendering="crispEdges" className="skin-head">
      {STEVE.flatMap((row, y) =>
        [...row].map((c, x) => <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} fill={P[c]} />),
      )}
    </svg>
  );
}

/** Face + hat layer cut straight out of the 64×64 skin texture with CSS. */
export function SkinHead({ skinUrl, size = 32 }: { skinUrl?: string | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  if (!skinUrl || failed) return <SteveFace size={size} />;

  const layer = (x: number) => ({
    backgroundImage: `url("${skinUrl}")`,
    backgroundSize: `${size * 8}px auto`,
    backgroundPosition: `-${size * x}px -${size}px`,
  });

  return (
    <span className="skin-head" style={{ width: size, height: size }}>
      {/* Hidden probe to detect load failures. */}
      <img src={skinUrl} alt="" hidden onError={() => setFailed(true)} />
      <span className="skin-layer" style={layer(1)} />
      <span className="skin-layer hat" style={layer(5)} />
    </span>
  );
}
