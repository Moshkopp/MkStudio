// Gemeinsame Kamera für alle GPU-Ansichten (ADR 0008): mm ↔ Bildschirm ↔ Clip.
//
// Bisher rechnete jeder Canvas (Design, Preview) seine eigene mm→Pixel-Logik.
// Diese Kamera bündelt sie: EINE Quelle für Zoom/Pan und die Umrechnungen, die
// sowohl das Frontend (Maus→mm) als auch der WebGL-Renderer (mm→Clip) brauchen.

/** Ansichts-Transform: Bildschirm-Pixel = mm * zoom + pan. */
export interface Camera {
  zoom: number;
  panX: number;
  panY: number;
}

/** mm → Bildschirm-Pixel (wie das alte `toScreen`). */
export function toScreen(cam: Camera, x: number, y: number): [number, number] {
  return [x * cam.zoom + cam.panX, y * cam.zoom + cam.panY];
}

/** Bildschirm-Pixel → mm (wie das alte `toMm`). */
export function toMm(cam: Camera, px: number, py: number): [number, number] {
  return [(px - cam.panX) / cam.zoom, (py - cam.panY) / cam.zoom];
}

/**
 * mm → WebGL-Clip-Space (−1..+1, y nach oben). Der Renderer nutzt das im
 * Vertex-Shader; hier als 3×3-Matrix (column-major für WebGL), die mm direkt
 * nach Clip abbildet — inklusive Zoom/Pan und der y-Umkehr (Bildschirm y↓,
 * Clip y↑). `w`/`h` sind die Canvas-Pixelmaße.
 */
export function mmToClipMatrix(cam: Camera, w: number, h: number): Float32Array {
  // Bildschirm-Pixel px = x*zoom + panX ; py = y*zoom + panY.
  // Clip cx = px/w*2 - 1 ; cy = 1 - py/h*2  (y umgekehrt).
  // Eingesetzt und nach x,y sortiert ergibt die affine Abbildung:
  const sx = (2 * cam.zoom) / w;
  const sy = (-2 * cam.zoom) / h;
  const tx = (2 * cam.panX) / w - 1;
  const ty = 1 - (2 * cam.panY) / h;
  // column-major 3×3: [ sx 0 0 ; 0 sy 0 ; tx ty 1 ]
  return new Float32Array([sx, 0, 0, 0, sy, 0, tx, ty, 1]);
}
