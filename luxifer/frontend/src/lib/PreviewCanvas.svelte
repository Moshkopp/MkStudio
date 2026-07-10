<script lang="ts">
  // Laser-Vorschau (ADR 0005): zeichnet die zu fahrenden Segmente in
  // Ausfuehrungsreihenfolge inkl. Verfahrwege. EIGENSTAENDIGER Canvas fuer den
  // Preview-Reiter — der Design-Canvas bleibt unangetastet. Die Segmente kommen
  // aus dem Core (jobPreview); das Frontend zeichnet nur (CLAUDE.md Regel 1/2).
  //
  // Rendering auf der GPU (ADR 0008): EIN Draw-Call pro Batch statt CPU-stroke()
  // pro Segment — ein Bild mit zehntausenden Rasterzeilen bleibt so flüssig.
  import { onMount } from "svelte";
  import { jobPreview, type JobPreview, type Scene } from "./core";
  import { GlRenderer, type LineBatch, type GlBatch, type GlTexture } from "./gl/renderer";
  import { toScreen as camToScreen, toMm as camToMm, type Camera } from "./gl/camera";
  import { previewToBatches } from "./gl/preview-render";

  interface Props {
    // Aktueller Zustand — liefert Bettgroesse und triggert das Neuladen der
    // Vorschau, wenn er sich aendert.
    scene: Scene;
    // Freie Raender wie beim Design-Canvas (Header oben, ggf. Panels).
    insets?: { top: number; right: number; bottom: number; left: number };
  }

  let { scene, insets }: Props = $props();

  const bedW = $derived(scene.bed_w_mm);
  const bedH = $derived(scene.bed_h_mm);

  let wrapEl: HTMLDivElement;
  let canvasEl: HTMLCanvasElement;
  let rafId = 0;
  let gl: GlRenderer | null = null;

  let preview = $state<JobPreview | null>(null);
  let loading = $state(false);
  // Verfahrwege (Leerfahrten) ein-/ausblendbar. Standard aus — bei vielen
  // Rasterzeilen ist die Leerfahrt-Wolke sonst dominant; der Nutzer blendet
  // sie bei Bedarf ein.
  let showTravel = $state(false);

  // Ansicht (wie Canvas.svelte).
  let zoom = $state(1.2);
  let panX = $state(40);
  let panY = $state(40);
  let viewTouched = false;

  const cam = (): Camera => ({ zoom, panX, panY });
  const toScreen = (x: number, y: number): [number, number] => camToScreen(cam(), x, y);
  const toMm = (px: number, py: number): [number, number] => camToMm(cam(), px, py);

  function fitBed() {
    if (!canvasEl) return;
    const cw = canvasEl.width, ch = canvasEl.height;
    const ins = insets ?? { top: 0, right: 0, bottom: 0, left: 0 };
    const availW = Math.max(50, cw - ins.left - ins.right);
    const availH = Math.max(50, ch - ins.top - ins.bottom);
    if (bedW <= 0 || bedH <= 0) return;
    const margin = 0.9;
    const nz = Math.min(availW / bedW, availH / bedH) * margin;
    const freeCx = ins.left + availW / 2;
    const freeCy = ins.top + availH / 2;
    const nx = freeCx - (bedW / 2) * nz;
    const ny = freeCy - (bedH / 2) * nz;
    if (
      Math.abs(nz - zoom) > 1e-6 ||
      Math.abs(nx - panX) > 1e-3 ||
      Math.abs(ny - panY) > 1e-3
    ) {
      zoom = nz; panX = nx; panY = ny;
    }
  }

  async function reload() {
    loading = true;
    try {
      preview = await jobPreview();
    } finally {
      loading = false;
    }
    rebuild(); // Daten neu → Batches + Texturen einmal hochladen
    draw();
  }

  function draw() {
    if (rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      render();
    });
  }

  // Hochgeladene GPU-Batches der Job-Geometrie. EINMAL bei Datenänderung gebaut
  // (rebuild), NICHT pro Frame — das ist der Kern der GPU-Performance: bei
  // Pan/Zoom ändert sich nur die Kamera-Matrix, die 79k Segmente bleiben oben.
  let workBatch: GlBatch | null = null;
  let travelBatch: GlBatch | null = null;
  let markerBatch: GlBatch | null = null;
  // Bild-Layer als hochgeladene GPU-Texturen (ADR 0008 §2).
  let texBatches: GlTexture[] = [];

  // Job-Daten neu auf die GPU laden (nur wenn sich preview/showTravel ändern).
  function rebuild() {
    if (!gl) return;
    if (workBatch) { gl.free(workBatch); workBatch = null; }
    if (travelBatch) { gl.free(travelBatch); travelBatch = null; }
    if (markerBatch) { gl.free(markerBatch); markerBatch = null; }
    for (const t of texBatches) gl.freeTexture(t);
    texBatches = [];
    if (!preview) return;

    // Bild-Texturen: Base64 → Bytes → GPU-Textur an ihrer mm-Box.
    for (const r of preview.rasters) {
      const bytes = b64ToBytes(r.pixels_b64);
      texBatches.push(gl.uploadTexture(bytes, r.width, r.height, r.rect));
    }

    if (preview.moves.length === 0) return;
    const { work, travel, markers } = previewToBatches(preview, showTravel);
    workBatch = gl.upload(work.positions, work.colors);
    if (travel.positions.length) travelBatch = gl.upload(travel.positions, travel.colors);
    if (markers.positions.length) markerBatch = gl.upload(markers.positions, markers.colors);
  }

  function render() {
    if (!canvasEl || !gl) return;
    // Kontextverlust: Renderer + Batches neu aufbauen.
    if (gl.isLost()) {
      try { gl = new GlRenderer(canvasEl); rebuild(); } catch { return; }
    }
    const w = canvasEl.width, h = canvasEl.height;
    gl.begin(cam(), w, h, [0.078, 0.082, 0.094]); // #141518

    // Grid/Bett sind winzig (Dutzende Linien) und hängen vom Sichtbereich ab →
    // pro Frame ok. Die Job-Batches/Texturen dagegen sind vorab hochgeladen.
    const grid = gridBatch(w, h);
    const grbatch = gl.upload(grid.positions, grid.colors);
    gl.drawBatch(grbatch, "lines");
    gl.free(grbatch);
    const bed = bedBatch();
    const bbatch = gl.upload(bed.positions, bed.colors);
    gl.drawBatch(bbatch, "lines");
    gl.free(bbatch);

    // Bild-Texturen (hell gebrannt) zuerst, dann Vektoren/Marker darüber.
    for (const t of texBatches) gl.drawTexture(t, [0.9, 0.92, 0.96]);
    if (travelBatch) gl.drawBatch(travelBatch, "lines");
    if (workBatch) gl.drawBatch(workBatch, "lines");
    if (markerBatch) gl.drawBatch(markerBatch, "points");
  }

  // Base64 → Uint8Array (Textur-Bytes). atob ist im WebView verfügbar.
  function b64ToBytes(b64: string): Uint8Array {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  // Grid als Line-Batch in mm (der Shader rechnet mm→Clip). Schrittweite so,
  // dass die Linien am Bildschirm nicht zu dicht liegen.
  function gridBatch(w: number, h: number): LineBatch {
    let step = 50;
    while (step * zoom < 8) step *= 2;
    const [tlx, tly] = toMm(0, 0);
    const [brx, bry] = toMm(w, h);
    const pos: number[] = [];
    for (let x = Math.floor(tlx / step) * step; x <= brx; x += step) {
      pos.push(x, tly, x, bry);
    }
    for (let y = Math.floor(tly / step) * step; y <= bry; y += step) {
      pos.push(tlx, y, brx, y);
    }
    return solidBatch(pos, [1, 1, 1, 0.06]);
  }

  // Bett-Rechteck (blau) + Ursprungsmarker (gelb, konstante Bildschirmgröße).
  function bedBatch(): LineBatch {
    const pos: number[] = [], col: number[] = [];
    const push = (x0: number, y0: number, x1: number, y1: number, c: number[]) => {
      pos.push(x0, y0, x1, y1);
      for (let k = 0; k < 2; k++) col.push(c[0], c[1], c[2], c[3]);
    };
    const blue = [0.35, 0.59, 0.86, 0.9];
    push(0, 0, bedW, 0, blue);
    push(bedW, 0, bedW, bedH, blue);
    push(bedW, bedH, 0, bedH, blue);
    push(0, bedH, 0, 0, blue);
    // Ursprungsmarker: 18 px in mm umgerechnet, damit konstant groß.
    const m = 18 / zoom;
    const gold = [0.94, 0.71, 0.24, 1];
    push(0, 0, m, 0, gold);
    push(0, 0, 0, m, gold);
    return { positions: new Float32Array(pos), colors: new Float32Array(col) };
  }

  // Hilfs-Batch: alle Segmente in einer Farbe.
  function solidBatch(pos: number[], rgba: number[]): LineBatch {
    const col = new Float32Array((pos.length / 2) * 4);
    for (let i = 0; i < pos.length / 2; i++) {
      col[i * 4] = rgba[0]; col[i * 4 + 1] = rgba[1];
      col[i * 4 + 2] = rgba[2]; col[i * 4 + 3] = rgba[3];
    }
    return { positions: new Float32Array(pos), colors: col };
  }

  // ---- Pan / Zoom (nur Ansicht, kein Bearbeiten) ---------------------------
  let panning: { px: number; py: number; ox: number; oy: number } | null = null;
  function localXY(ev: PointerEvent | WheelEvent): [number, number] {
    const r = canvasEl.getBoundingClientRect();
    return [ev.clientX - r.left, ev.clientY - r.top];
  }
  function onPointerDown(ev: PointerEvent) {
    canvasEl.setPointerCapture(ev.pointerId);
    const [px, py] = localXY(ev);
    panning = { px, py, ox: panX, oy: panY };
  }
  function onPointerMove(ev: PointerEvent) {
    if (!panning) return;
    viewTouched = true;
    const [px, py] = localXY(ev);
    panX = panning.ox + (px - panning.px);
    panY = panning.oy + (py - panning.py);
    draw();
  }
  function onPointerUp(ev: PointerEvent) {
    if (panning) { canvasEl.releasePointerCapture(ev.pointerId); panning = null; }
  }
  function onWheel(ev: WheelEvent) {
    ev.preventDefault();
    viewTouched = true;
    const [px, py] = localXY(ev);
    const [wx, wy] = toMm(px, py);
    zoom = Math.max(0.05, Math.min(40, zoom * (ev.deltaY < 0 ? 1.15 : 0.85)));
    panX = px - wx * zoom;
    panY = py - wy * zoom;
    draw();
  }
  function resetView() {
    viewTouched = false;
    fitBed();
    draw();
  }

  function resize() {
    if (!wrapEl || !canvasEl) return;
    const nw = wrapEl.clientWidth, nh = wrapEl.clientHeight;
    if (canvasEl.width !== nw || canvasEl.height !== nh) {
      canvasEl.width = nw;
      canvasEl.height = nh;
    }
    if (!viewTouched) fitBed();
    draw();
  }

  onMount(() => {
    try {
      gl = new GlRenderer(canvasEl);
    } catch (e) {
      console.error("WebGL-Init fehlgeschlagen:", e);
    }
    resize();
    const ro = new ResizeObserver(resize);
    if (wrapEl) ro.observe(wrapEl);
    return () => { ro.disconnect(); if (rafId) cancelAnimationFrame(rafId); };
  });

  // Bei Sichtbarwerden / Zustandsaenderung neu vom Core laden.
  $effect(() => { scene; reload(); });
  // Travel ein-/ausblenden ändert die Daten → Batches neu hochladen.
  $effect(() => { showTravel; rebuild(); draw(); });
  // Reines Redraw bei Ansichtsänderung (Zoom/Pan) — KEIN Batch-Neuaufbau.
  $effect(() => { zoom; panX; panY; insets; draw(); });

  // Kurz-Statistik fuer die Legende.
  const cutCount = $derived(preview?.moves.filter((m) => m.kind === "Cut").length ?? 0);
  const fillCount = $derived(preview?.moves.filter((m) => m.kind === "Fill").length ?? 0);
  const travelCount = $derived(preview?.moves.filter((m) => m.kind === "Travel").length ?? 0);
  const totalLen = $derived(preview?.total_len_mm ?? 0);
</script>

<div class="wrap" bind:this={wrapEl}>
  <canvas
    bind:this={canvasEl}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onwheel={onWheel}
  ></canvas>

  <!-- Info-/Steuerleiste unten links. Play/Scrubber folgt spaeter (ADR 0005). -->
  <div class="bar glass">
    {#if loading}
      <span class="muted">Vorschau wird berechnet…</span>
    {:else if !preview || (preview.moves.length === 0 && preview.rasters.length === 0)}
      <span class="muted">Kein Job — nichts zu fahren.</span>
    {:else}
      <span class="stat"><span class="dot start"></span>Start</span>
      <span class="stat"><span class="dot end"></span>Ende</span>
      <span class="sep"></span>
      <span class="stat">{cutCount} Schnitt</span>
      <span class="stat">{fillCount} Füllung</span>
      <label class="stat travel">
        <input type="checkbox" bind:checked={showTravel} />
        {travelCount} Leerfahrt
      </label>
      <span class="sep"></span>
      <span class="stat">{(totalLen / 1000).toFixed(2)} m Weg</span>
    {/if}
    <span class="grow"></span>
    <button class="mini" onclick={resetView} title="Ansicht zuruecksetzen">Ansicht</button>
  </div>
</div>

<style>
  .wrap { position: absolute; inset: 0; }
  canvas {
    display: block;
    touch-action: none;
    transform: translateZ(0);
    will-change: transform;
    cursor: grab;
  }
  canvas:active { cursor: grabbing; }

  .bar {
    position: absolute;
    left: 16px;
    bottom: 16px;
    right: 16px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-radius: 12px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.85);
    pointer-events: auto;
  }
  .glass {
    background: rgba(20, 22, 26, 0.62);
    backdrop-filter: blur(14px) saturate(1.2);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.35);
  }
  .muted { color: rgba(255, 255, 255, 0.5); }
  .stat { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .travel { cursor: pointer; user-select: none; }
  .travel input { accent-color: #6ea8ff; }
  .dot { width: 10px; height: 10px; border-radius: 50%; display: inline-block; }
  .dot.start { background: #3fb27f; }
  .dot.end { background: #ff5c62; }
  .sep { width: 1px; height: 18px; background: rgba(255, 255, 255, 0.14); }
  .grow { flex: 1; }
  .mini {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.85);
    border-radius: 8px;
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .mini:hover { background: rgba(255, 255, 255, 0.14); }
</style>
