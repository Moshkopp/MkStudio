<script lang="ts">
  // Anordnen-Toolbar: Ausrichten/Verteilen + Nesting. Die Geometrie-Werkzeuge
  // (Boolean/Fillet/Offset/Muster) liegen wie in der Referenz in der
  // WERKZEUGLEISTE (Gruppe 3), nicht hier.
  import type { AlignKind, DistributeKind } from "./core";
  let {
    selCount,
    onalign,
    ondistribute,
    onnest,
    onnestfill,
    ongroup,
    onungroup,
    selBBox,
    onresize,
  }: {
    selCount: number;
    onalign: (k: AlignKind) => void;
    ondistribute: (k: DistributeKind) => void;
    onnest: (gap: number) => void;
    onnestfill: (gap: number) => void;
    ongroup: () => void;
    onungroup: () => void;
    // BBox der Auswahl (x,y,w,h in mm) oder null.
    selBBox: [number, number, number, number] | null;
    // Auswahl auf neue Breite/Höhe skalieren (Anker = linke Oberkante).
    onresize: (w: number, h: number) => void;
  } = $props();

  // Nest-Abstand (mm).
  let nestGapMm = $state(2.0);

  // Größe der Auswahl per Eingabe (mit Seitenverhältnis-Sperre).
  let ratioLock = $state(true);
  let wIn = $state(0);
  let hIn = $state(0);
  // Eingaben folgen der Auswahl (aber nicht während man tippt — Werte werden
  // beim Auswahlwechsel gesetzt).
  $effect(() => {
    if (selBBox) {
      wIn = Math.round(selBBox[2] * 100) / 100;
      hIn = Math.round(selBBox[3] * 100) / 100;
    }
  });
  function applyW() {
    if (!selBBox || wIn <= 0) return;
    const h = ratioLock ? wIn * (selBBox[3] / selBBox[2]) : hIn;
    onresize(wIn, h);
  }
  function applyH() {
    if (!selBBox || hIn <= 0) return;
    const w = ratioLock ? hIn * (selBBox[2] / selBBox[3]) : wIn;
    onresize(w, hIn);
  }
</script>

<div class="col">
  <div class="arrange">
    <button class="gbtn" disabled={selCount < 2} onclick={() => onalign("left")} title="Links ausrichten">⇤</button>
    <button class="gbtn" disabled={selCount < 2} onclick={() => onalign("hcenter")} title="Horizontal zentrieren">⇔</button>
    <button class="gbtn" disabled={selCount < 2} onclick={() => onalign("right")} title="Rechts ausrichten">⇥</button>
    <div class="vsep"></div>
    <button class="gbtn" disabled={selCount < 2} onclick={() => onalign("top")} title="Oben ausrichten">⤒</button>
    <button class="gbtn" disabled={selCount < 2} onclick={() => onalign("vcenter")} title="Vertikal zentrieren">⇕</button>
    <button class="gbtn" disabled={selCount < 2} onclick={() => onalign("bottom")} title="Unten ausrichten">⤓</button>
    <div class="vsep"></div>
    <button class="gbtn" disabled={selCount < 3} onclick={() => ondistribute("h")} title="Horizontal verteilen">⋯</button>
    <button class="gbtn" disabled={selCount < 3} onclick={() => ondistribute("v")} title="Vertikal verteilen">⋮</button>
    <div class="vsep"></div>
    <button class="gbtn" disabled={selCount < 2} onclick={ongroup} title="Gruppieren (Strg+G)">⧉</button>
    <button class="gbtn" disabled={selCount < 1} onclick={onungroup} title="Gruppierung lösen (Strg+Umschalt+G)">⧎</button>
  </div>

  <!-- Größe der Auswahl numerisch setzen (Ratio-Lock koppelt B/H). -->
  <div class="arrange">
    <span class="lbl">B</span>
    <input class="mm" type="number" step="0.5" min="0.1" bind:value={wIn}
      disabled={!selBBox} onchange={applyW} onkeydown={(e) => e.key === "Enter" && applyW()} title="Breite in mm" />
    <button class="gbtn" class:on={ratioLock} onclick={() => (ratioLock = !ratioLock)}
      title={ratioLock ? "Seitenverhältnis gesperrt" : "Seitenverhältnis frei"}>{ratioLock ? "🔒" : "🔓"}</button>
    <span class="lbl">H</span>
    <input class="mm" type="number" step="0.5" min="0.1" bind:value={hIn}
      disabled={!selBBox} onchange={applyH} onkeydown={(e) => e.key === "Enter" && applyH()} title="Höhe in mm" />
  </div>

  <!-- Nesting: Auswahl platzsparend aufs Bett packen. -->
  <div class="arrange">
    <input class="mm" type="number" step="0.5" min="0" bind:value={nestGapMm} title="Abstand zwischen den Teilen in mm" />
    <button class="gbtn wide grow" disabled={selCount < 2} onclick={() => onnest(nestGapMm)} title="Auswahl platzsparend packen">
      Nest
    </button>
    <button class="gbtn wide grow" disabled={selCount < 1} onclick={() => onnestfill(nestGapMm)} title="Bett mit Kopien der ersten ausgewählten Form füllen">
      Bett füllen
    </button>
  </div>
</div>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  /* Einreihig; die Buttons teilen sich die Panelbreite und passen sich ihr an
     (kein Umbruch, kein Stauchen). So bleibt die Reihe intakt, egal wie schmal
     das Panel gezogen wird. */
  .arrange {
    display: flex;
    align-items: center;
    gap: 3px;
    width: 100%;
    container-type: inline-size;
  }
  button {
    flex: 1 1 0;
    min-width: 0;
    /* Quadratisch: Hoehe folgt der (mit dem Panel schrumpfenden) Breite,
       gedeckelt, damit die Buttons in breiten Panels nicht riesig werden. */
    aspect-ratio: 1;
    max-width: 34px;
    /* Icon-/Glyphgroesse skaliert mit der Buttonbreite. */
    font-size: clamp(10px, 2.6cqw, 16px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  button.wide {
    aspect-ratio: auto;
    max-width: 52px;
    padding: 0 6px;
    font-size: 11px;
  }
  button.grow {
    max-width: none;
    height: 26px;
  }
  .mm {
    flex: 1 1 0;
    min-width: 34px;
    max-width: 56px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-size: 12px;
    padding: 4px 4px;
  }
  .lbl {
    font-size: 11px;
    color: var(--muted);
    flex: none;
    padding: 0 2px;
  }
  button.on {
    border-color: var(--accent);
  }
  .vsep {
    flex: 0 0 1px;
    align-self: stretch;
    background: var(--border);
    margin: 3px 4px;
  }
</style>
