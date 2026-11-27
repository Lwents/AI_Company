import { useEffect, useRef } from "react";
import type { Employee } from "@/api/types";
import { characterSprite, lookFor } from "@/features/office/sprites/character";

/** The employee's office character (head and shoulders) on their colour. */
export function Avatar({ e, small }: { e: Employee; small?: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const look = lookFor(e.id, e.name, e.color, e.role, e.gender);
  const key = JSON.stringify(look);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const sprite = characterSprite(look, "stand", 1);
    const ctx = c.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, c.width, c.height);
    // Crop the top 16 rows (hair to chest) of the 16-wide sprite.
    ctx.drawImage(sprite, 0, 0, 16, 16, 0, 0, c.width, c.height);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return (
    <span className={`avatar char${small ? " sm" : ""}`} style={{ background: e.color }} title={e.name}>
      <canvas ref={ref} width={32} height={32} aria-hidden="true" />
    </span>
  );
}
