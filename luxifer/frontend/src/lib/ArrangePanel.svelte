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
  let nestOpen = $state(false);
  let nestMode = $state<"pack" | "fill">("pack");

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
  function canApplyNest(): boolean {
    return nestMode === "fill" ? selCount >= 1 : selCount >= 2;
  }
  function applyNest() {
    if (!canApplyNest()) return;
    if (nestMode === "fill") onnestfill(nestGapMm);
    else onnest(nestGapMm);
    nestOpen = false;
  }
</script>

<div class="toolbar">
  <div class="group">
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
  <div class="group size-group">
    <span class="lbl">B</span>
    <input class="mm" type="number" step="0.5" min="0.1" bind:value={wIn}
      disabled={!selBBox} onchange={applyW} onkeydown={(e) => e.key === "Enter" && applyW()} title="Breite in mm" />
    <button class="gbtn" class:on={ratioLock} onclick={() => (ratioLock = !ratioLock)}
      title={ratioLock ? "Seitenverhältnis gesperrt" : "Seitenverhältnis frei"}>{ratioLock ? "🔒" : "🔓"}</button>
    <span class="lbl">H</span>
    <input class="mm" type="number" step="0.5" min="0.1" bind:value={hIn}
      disabled={!selBBox} onchange={applyH} onkeydown={(e) => e.key === "Enter" && applyH()} title="Höhe in mm" />
  </div>

  <div class="nest-wrap">
    <button class="gbtn wide" disabled={selCount < 1} onclick={() => (nestOpen = !nestOpen)} title="Nesting-Optionen">
      Nesting
    </button>
    {#if nestOpen}
      <div class="nest-menu glass">
        <label>
          Modus
          <select bind:value={nestMode}>
            <option value="pack">Auswahl packen</option>
            <option value="fill">Bett füllen</option>
          </select>
        </label>
        <label>
          Abstand
          <input class="mm" type="number" step="0.5" min="0" bind:value={nestGapMm} />
        </label>
        <button class="gbtn primary" disabled={!canApplyNest()} onclick={applyNest}>Fertig</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    container-type: inline-size;
  }
  .size-group {
    margin-left: auto;
    display: grid;
    grid-template-columns: auto 58px 30px auto 58px;
    gap: 4px;
    flex: 0 0 auto;
  }
  button {
    flex: 0 0 30px;
    aspect-ratio: 1;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  button.wide {
    aspect-ratio: auto;
    flex-basis: auto;
    height: 30px;
    padding: 0 12px;
    font-size: 12px;
  }
  .mm {
    min-width: 34px;
    width: 58px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-size: 12px;
    padding: 4px 4px;
    text-align: right;
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
  button.primary {
    width: 100%;
    flex: none;
    aspect-ratio: auto;
    background: var(--accent);
    color: white;
    height: 30px;
  }
  .vsep {
    flex: 0 0 1px;
    height: 24px;
    background: var(--border);
    margin: 3px 4px;
  }
  .nest-wrap {
    position: relative;
    flex: 0 0 auto;
  }
  .nest-menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    width: 190px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 80;
  }
  .nest-menu label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font-size: 11px;
  }
  select {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 5px 7px;
  }
</style>
