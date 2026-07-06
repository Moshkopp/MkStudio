<script lang="ts">
  import Canvas from "./lib/Canvas.svelte";
  import * as core from "./lib/core";
  import type { Scene } from "./lib/core";

  type Tool = "select" | "rect" | "ellipse";

  let scene = $state<Scene | null>(null);
  let tool = $state<Tool>("rect");
  let swatches = $state<[number, number, number][]>([]);
  let error = $state<string | null>(null);

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
      <span class="label">Ebenen</span>
      {#each scene.layers as l}
        <div class="layer">
          <span class="chip" style="background: {rgb(l.color)}"></span>
          <span>{l.name}</span>
          <span class="muted">{l.mode}</span>
        </div>
      {/each}
      {#if scene.layers.length === 0}
        <div class="muted">— noch leer —</div>
      {/if}
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
  }
  .chip {
    width: 14px;
    height: 14px;
    border-radius: 4px;
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
</style>
