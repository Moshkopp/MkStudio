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
    ongroup,
    onungroup,
  }: {
    selCount: number;
    onalign: (k: AlignKind) => void;
    ondistribute: (k: DistributeKind) => void;
    onnest: (gap: number) => void;
    ongroup: () => void;
    onungroup: () => void;
  } = $props();

  // Nest-Abstand (mm).
  let nestGapMm = $state(2.0);
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

  <!-- Nesting: Auswahl platzsparend aufs Bett packen. -->
  <div class="arrange">
    <input class="mm" type="number" step="0.5" min="0" bind:value={nestGapMm} title="Abstand zwischen den Teilen in mm" />
    <button class="gbtn wide grow" disabled={selCount < 2} onclick={() => onnest(nestGapMm)} title="Auswahl platzsparend packen (Material-Ausnutzung)">
      Verschachteln (Nest)
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
  .vsep {
    flex: 0 0 1px;
    align-self: stretch;
    background: var(--border);
    margin: 3px 4px;
  }
</style>
