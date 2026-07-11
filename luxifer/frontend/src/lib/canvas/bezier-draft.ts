// Bézier-Feder (Zeichenentwurf): reine Kurven-Abtastung der noch nicht
// abgeschlossenen Knotenkette. UI-frei — Canvas.svelte zeichnet das Ergebnis.
// Die maßgebliche Kurve entsteht beim Abschluss im Rust-Core; dies ist nur die
// Live-Vorschau während des Zeichnens.

/** Ein Entwurfs-Knoten: Anker + optionale Ein-/Ausgangstangenten (mm). */
export type BNode = { p: [number, number]; hIn: [number, number] | null; hOut: [number, number] | null };

/** Tastet ein kubisches Segment p0..p3 mit N Stützpunkten ab (hängt an `out` an). */
function cubic(
  p0: [number, number],
  p1: [number, number],
  p2: [number, number],
  p3: [number, number],
  out: [number, number][],
) {
  const N = 16;
  for (let i = 1; i <= N; i++) {
    const t = i / N, u = 1 - t;
    out.push([
      u * u * u * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t * t * t * p3[0],
      u * u * u * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t * t * t * p3[1],
    ]);
  }
}

/**
 * Flacht die Knotenkette zu einer Polylinie ab. Fehlende Tangenten werden zum
 * Anker degeneriert (gerade Segmente). `closed` schließt letzten→ersten Knoten.
 */
export function bezFlatten(nodes: BNode[], closed: boolean): [number, number][] {
  if (nodes.length < 2) return nodes.map((n) => n.p);
  const out: [number, number][] = [nodes[0].p];
  const seg = (a: BNode, b: BNode) => cubic(a.p, a.hOut ?? a.p, b.hIn ?? b.p, b.p, out);
  for (let i = 0; i < nodes.length - 1; i++) seg(nodes[i], nodes[i + 1]);
  if (closed) seg(nodes[nodes.length - 1], nodes[0]);
  return out;
}
