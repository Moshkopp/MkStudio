// Auswahl-Handles: reine Geometrie der 8 Griffe einer Bounding-Box und das
// Resize-Verhalten. UI-frei (keine Kamera, kein Zustand) — nimmt/gibt mm-Boxen.
// Canvas.svelte nutzt das für Hit-Test der Griffe und die Live-Skalierung.

export type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

/** Die 8 Griffpositionen (mm) einer Box [x, y, w, h] mit ihrer Kennung. */
export function handlePositions(box: [number, number, number, number]): [HandleId, number, number][] {
  const [x, y, w, h] = box;
  const cx = x + w / 2, cy = y + h / 2;
  return [
    ["nw", x, y], ["n", cx, y], ["ne", x + w, y],
    ["e", x + w, cy], ["se", x + w, y + h], ["s", cx, y + h],
    ["sw", x, y + h], ["w", x, cy],
  ];
}

/**
 * Neue Box beim Ziehen eines Griffs um (dx, dy). Eck-Griffe wahren das
 * Seitenverhältnis (verankert an der Gegenecke); Kanten-Griffe skalieren nur
 * eine Achse. Mindestgröße 0.1 mm, damit die Box nicht kollabiert.
 */
export function resizeBox(
  start: [number, number, number, number],
  handle: HandleId,
  dx: number,
  dy: number,
): [number, number, number, number] {
  let [x, y, w, h] = start;
  const left = handle === "w" || handle === "nw" || handle === "sw";
  const right = handle === "e" || handle === "ne" || handle === "se";
  const top = handle === "n" || handle === "nw" || handle === "ne";
  const bottom = handle === "s" || handle === "sw" || handle === "se";
  const corner = (left || right) && (top || bottom);
  if (corner) {
    // Eck-Handle: Seitenverhaeltnis wahren. Ein gemeinsamer Faktor aus dem
    // groesseren relativen Delta, verankert an der gegenueberliegenden Ecke.
    const sw0 = start[2], sh0 = start[3];
    const fx = 1 + (right ? dx : -dx) / sw0;
    const fy = 1 + (bottom ? dy : -dy) / sh0;
    const f = Math.max(0.001, Math.max(fx, fy)); // dominante Achse fuehrt
    w = sw0 * f;
    h = sh0 * f;
    // Gegenueberliegende Ecke fix halten.
    if (left) x = start[0] + start[2] - w;
    if (top) y = start[1] + start[3] - h;
  } else {
    if (left) { x += dx; w -= dx; }
    if (right) { w += dx; }
    if (top) { y += dy; h -= dy; }
    if (bottom) { h += dy; }
  }
  if (w < 0.1) w = 0.1;
  if (h < 0.1) h = 0.1;
  return [x, y, w, h];
}

/** Referenz-x-Kante des Griffs in der Startbox (für ein konsistentes Delta). */
export function hxOffset(h: HandleId, b: [number, number, number, number]): number {
  if (h === "e" || h === "ne" || h === "se") return b[2];
  if (h === "n" || h === "s") return b[2] / 2;
  return 0;
}

/** Referenz-y-Kante des Griffs in der Startbox. */
export function hyOffset(h: HandleId, b: [number, number, number, number]): number {
  if (h === "s" || h === "sw" || h === "se") return b[3];
  if (h === "e" || h === "w") return b[3] / 2;
  return 0;
}
