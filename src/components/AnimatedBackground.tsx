/** Slowly drifting, blurred colour blobs (GPU-only transforms). */
export function AnimatedBackground() {
  return (
    <div className="bg" aria-hidden>
      <div className="bg-blob b1" />
      <div className="bg-blob b2" />
      <div className="bg-blob b3" />
      <div className="bg-blob b4" />
      <div className="bg-noise" />
    </div>
  );
}
