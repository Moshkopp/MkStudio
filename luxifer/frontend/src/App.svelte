<script lang="ts">
  import Canvas from "./lib/Canvas.svelte";
  import LayerDialog from "./lib/LayerDialog.svelte";
  import LaserPanel from "./lib/LaserPanel.svelte";
  import * as core from "./lib/core";
  import type { Scene, LayerParams } from "./lib/core";

  type Tool = "select" | "rect" | "ellipse";

  let scene = $state<Scene | null>(null);
  let tool = $state<Tool>("rect");
  let swatches = $state<[number, number, number][]>([]);
  let error = $state<string | null>(null);
  // Index des Layers, dessen Dialog offen ist (oder null).
  let editLayer = $state<number | null>(null);
  // Erzeugter G-Code (Overlay), oder null.
  let gcode = $state<string | null>(null);

  async function load() {
    try {
      scene = await core.getScene();
      swatches = await core.swatchColors();
    } catch (e) {
      error = String(e);
    }
  }
  load();

  // --- Canvas-Callbacks (Interaktion geschieht im Canvas, Aktionen im Core) ---
  async function ondrawrect(x: number, y: number, w: number, h: number) {
    scene = await core.addRect(x, y, w, h);
  }
  async function ondrawellipse(cx: number, cy: number, rx: number, ry: number) {
    scene = await core.addEllipse(cx, cy, rx, ry);
  }
  async function onselectat(x: number, y: number, additive: boolean) {
    scene = await core.selectAt(x, y, 2, additive);
  }
  async function onselectrect(x1: number, y1: number, x2: number, y2: number) {
    scene = await core.selectRect(x1, y1, x2, y2);
  }
  async function onmove(dx: number, dy: number) {
    scene = await core.moveSelected(dx, dy);
  }
  async function onscale(
    start: [number, number, number, number],
    target: [number, number, number, number],
  ) {
    scene = await core.scaleSelected(start, target);
  }

  async function pickColor(c: [number, number, number]) {
    scene = await core.activateColor(c);
  }

  async function doAlign(kind: core.AlignKind) {
    scene = await core.align(kind);
  }
  async function doDistribute(kind: core.DistributeKind) {
    scene = await core.distribute(kind);
  }

  async function saveLayer(p: LayerParams) {
    if (editLayer !== null) {
      scene = await core.setLayerParams(editLayer, p);
      editLayer = null;
    }
  }
  async function toggleLayer(i: number, field: "visible" | "locked") {
    scene = await core.toggleLayer(i, field);
  }

  async function generateGcode() {
    try {
      gcode = await core.generateGcode();
    } catch (e) {
      error = String(e);
    }
  }
  function copyGcode() {
    if (gcode) navigator.clipboard?.writeText(gcode);
  }

  const selCount = $derived(scene?.selected.length ?? 0);

  async function doUndo() {
    scene = await core.undo();
  }
  async function doRedo() {
    scene = await core.redo();
  }
  async function doDelete() {
    scene = await core.deleteSelected();
  }

  const rgb = core.rgb;
</script>

<main>
  {#if error}
    <div class="error">Fehler: {error}</div>
  {/if}

  {#if scene}
    <Canvas
      {scene}
      {tool}
      {ondrawrect}
      {ondrawellipse}
      {onselectat}
      {onselectrect}
      {onmove}
      {onscale}
    />
  {/if}

  <!-- Anordnen-Toolbar oben mittig (immer sichtbar; Knöpfe je nach Auswahl aktiv) -->
  <div class="panel arrange">
    <button disabled={selCount < 2} onclick={() => doAlign("left")} title="Links ausrichten">⇤</button>
    <button disabled={selCount < 2} onclick={() => doAlign("hcenter")} title="Horizontal zentrieren">⇔</button>
    <button disabled={selCount < 2} onclick={() => doAlign("right")} title="Rechts ausrichten">⇥</button>
    <div class="vsep"></div>
    <button disabled={selCount < 2} onclick={() => doAlign("top")} title="Oben ausrichten">⤒</button>
    <button disabled={selCount < 2} onclick={() => doAlign("vcenter")} title="Vertikal zentrieren">⇕</button>
    <button disabled={selCount < 2} onclick={() => doAlign("bottom")} title="Unten ausrichten">⤓</button>
    <div class="vsep"></div>
    <button disabled={selCount < 3} onclick={() => doDistribute("h")} title="Horizontal verteilen">⋯</button>
    <button disabled={selCount < 3} onclick={() => doDistribute("v")} title="Vertikal verteilen">⋮</button>
  </div>

  <!-- Werkzeugleiste links -->
  <div class="panel tools">
    <button class:active={tool === "select"} onclick={() => (tool = "select")} title="Auswählen">▲</button>
    <button class:active={tool === "rect"} onclick={() => (tool = "rect")} title="Rechteck">▭</button>
    <button class:active={tool === "ellipse"} onclick={() => (tool = "ellipse")} title="Ellipse">◯</button>
    <div class="sep"></div>
    <button onclick={doUndo} title="Rückgängig">↶</button>
    <button onclick={doRedo} title="Wiederholen">↷</button>
    <button onclick={doDelete} title="Löschen">🗑</button>
  </div>

  <!-- Farbpalette unten -->
  <div class="panel palette">
    <span class="label">Farbe</span>
    {#each swatches as c}
      <button
        class="swatch"
        style="background: {rgb(c)}"
        title={rgb(c)}
        onclick={() => pickColor(c)}
        aria-label={rgb(c)}
      ></button>
    {/each}
  </div>

  <!-- Ebenen rechts -->
  {#if scene}
    <div class="panel layers">
      <span class="label">Ebenen · Doppelklick bearbeitet</span>
      {#each scene.layers as l, i}
        <div
          class="layer"
          ondblclick={() => (editLayer = i)}
          onkeydown={(e) => e.key === "Enter" && (editLayer = i)}
          role="button"
          tabindex="0"
        >
          <span class="chip" style="background: {rgb(l.color)}"></span>
          <div class="layer-info">
            <span>{l.name}</span>
            <span class="muted">{l.mode} · {l.speed_mm_s} mm/s · {l.power_pct}%</span>
          </div>
          <button
            class="mini"
            class:off={!l.visible}
            title="Sichtbar"
            onclick={(e) => { e.stopPropagation(); toggleLayer(i, "visible"); }}
          >{l.visible ? "👁" : "◠"}</button>
          <button
            class="mini"
            class:on={l.locked}
            title="Sperre"
            onclick={(e) => { e.stopPropagation(); toggleLayer(i, "locked"); }}
          >{l.locked ? "🔒" : "🔓"}</button>
        </div>
      {/each}
      {#if scene.layers.length === 0}
        <div class="muted">— noch leer —</div>
      {/if}
    </div>
  {/if}

  {#if scene && editLayer !== null && scene.layers[editLayer]}
    <LayerDialog
      layer={scene.layers[editLayer]}
      onsave={saveLayer}
      oncancel={() => (editLayer = null)}
    />
  {/if}

  <!-- Laser-Control-Panel unten rechts -->
  <LaserPanel ongenerate={generateGcode} />

  <!-- G-Code-Overlay -->
  {#if gcode !== null}
    <div
      class="backdrop"
      onclick={() => (gcode = null)}
      onkeydown={(e) => e.key === "Escape" && (gcode = null)}
      role="button"
      tabindex="-1"
    >
      <div class="gcode" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
        <div class="gc-head">
          <span>G-Code ({gcode.split("\n").length} Zeilen)</span>
          <div>
            <button onclick={copyGcode}>Kopieren</button>
            <button class="primary" onclick={() => (gcode = null)}>Schließen</button>
          </div>
        </div>
        <pre>{gcode}</pre>
      </div>
    </div>
  {/if}
</main>

<style>
  main {
    position: absolute;
    inset: 0;
  }
  .panel {
    position: absolute;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 40px -4px rgba(0, 0, 0, 0.5);
    padding: 8px;
    z-index: 10;
  }
  .tools {
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tools button {
    width: 40px;
    height: 40px;
    font-size: 18px;
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 2px;
  }
  .arrange {
    left: 50%;
    top: 12px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .arrange button {
    width: 34px;
    height: 34px;
    font-size: 16px;
  }
  .vsep {
    width: 1px;
    align-self: stretch;
    background: var(--border);
    margin: 3px 4px;
  }
  .palette {
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 11px;
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
  }
  .swatch:hover {
    border-color: white;
    transform: scale(1.15);
  }
  .layers {
    right: 12px;
    top: 12px;
    width: 220px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .layer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
  }
  .layer:hover {
    background: #26282d;
  }
  .layer-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }
  .layer-info span:first-child {
    font-weight: 500;
  }
  .layer-info .muted {
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mini {
    width: 26px;
    height: 26px;
    padding: 0;
    font-size: 13px;
    background: transparent;
  }
  .mini.off {
    opacity: 0.4;
  }
  .mini.on {
    color: var(--accent);
  }
  .chip {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    flex-shrink: 0;
  }
  .label {
    font-size: 11px;
    letter-spacing: 1px;
    color: var(--muted);
    text-transform: uppercase;
  }
  .muted {
    color: var(--muted);
  }
  button {
    background: #26282d;
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 6px 10px;
    cursor: pointer;
    transition: background 0.14s;
  }
  button:hover {
    background: #2e3036;
  }
  button.active {
    background: var(--accent);
    color: white;
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  button:disabled:hover {
    background: #26282d;
  }
  .error {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    background: #331e1e;
    color: #e5645d;
    padding: 6px 12px;
    border-radius: 8px;
    z-index: 20;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .gcode {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 14px;
    width: min(600px, 90%);
    max-height: 80%;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px -8px rgba(0, 0, 0, 0.6);
  }
  .gc-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
    gap: 8px;
  }
  .gc-head > div {
    display: flex;
    gap: 8px;
  }
  .gcode pre {
    margin: 0;
    padding: 14px 16px;
    overflow: auto;
    font-family: ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text);
  }
  .primary {
    background: var(--accent);
    color: white;
  }
</style>
