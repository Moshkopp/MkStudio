// Wandelt die Core-Preview (JobPreview) in GPU-Line-Batches (ADR 0008).
// Reihenfolge-Verlauf (früh kühl → spät warm) als Per-Vertex-Farbe, damit alles
// in EINEM Draw-Call bleibt. Reine Datenumformung (testbar, UI-frei).

import type { JobPreview, PreviewMove } from "../core";
import type { LineBatch } from "./renderer";

/** Arbeitsfarbe (einfarbig, ruhiges Hellblau). RGBA 0..1. */
const WORK_COLOR = [0.55, 0.72, 1.0, 1.0];

/** Ergebnis der Aufteilung: getrennte Batches für Arbeit und Travel. */
export interface PreviewBatches {
  work: LineBatch;
  travel: LineBatch;
  /** Start-/End-Marker (Positionen mm + Farben), für `points`. */
  markers: { positions: Float32Array; colors: Float32Array };
}

/**
 * Baut aus der Preview die GPU-Batches: Arbeit (Cut/Fill/Raster) im
 * Reihenfolge-Verlauf, Travel blass. Ein Batch = ein Draw-Call.
 * `showTravel` steuert, ob Travel-Segmente überhaupt erzeugt werden.
 */
export function previewToBatches(preview: JobPreview, showTravel: boolean): PreviewBatches {
  const moves = preview.moves;

  let workN = 0, travelN = 0;
  for (const m of moves) {
    if (m.kind === "Travel") travelN++;
    else workN++;
  }

  const work: LineBatch = {
    positions: new Float32Array(workN * 4),
    colors: new Float32Array(workN * 8),
  };
  const travel: LineBatch = {
    positions: new Float32Array((showTravel ? travelN : 0) * 4),
    colors: new Float32Array((showTravel ? travelN : 0) * 8),
  };

  let wi = 0, ti = 0;
  let firstWork: PreviewMove | null = null, lastWork: PreviewMove | null = null;
  for (const m of moves) {
    if (m.kind === "Travel") {
      if (!showTravel) continue;
      putSeg(travel, ti++, m, [1, 1, 1, 0.22]);
    } else {
      // Einfarbig (kein Reihenfolge-Verlauf) — schlicht und ruhig.
      putSeg(work, wi++, m, WORK_COLOR);
      if (!firstWork) firstWork = m;
      lastWork = m;
    }
  }

  // Marker: Start (grün) an firstWork.from, Ende (rot) an lastWork.to.
  const mp: number[] = [], mc: number[] = [];
  if (firstWork) {
    mp.push(firstWork.from[0], firstWork.from[1]);
    mc.push(0.25, 0.7, 0.5, 1);
  }
  if (lastWork) {
    mp.push(lastWork.to[0], lastWork.to[1]);
    mc.push(1, 0.36, 0.38, 1);
  }

  return {
    work,
    travel,
    markers: { positions: new Float32Array(mp), colors: new Float32Array(mc) },
  };
}

function putSeg(batch: LineBatch, i: number, m: PreviewMove, rgba: number[]) {
  const p = i * 4, c = i * 8;
  batch.positions[p] = m.from[0];
  batch.positions[p + 1] = m.from[1];
  batch.positions[p + 2] = m.to[0];
  batch.positions[p + 3] = m.to[1];
  // Beide Vertices des Segments in derselben Farbe.
  for (let k = 0; k < 2; k++) {
    batch.colors[c + k * 4] = rgba[0];
    batch.colors[c + k * 4 + 1] = rgba[1];
    batch.colors[c + k * 4 + 2] = rgba[2];
    batch.colors[c + k * 4 + 3] = rgba[3];
  }
}
