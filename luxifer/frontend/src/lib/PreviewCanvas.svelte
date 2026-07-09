<script lang="ts">
  // Laser-Vorschau (ADR 0005): zeichnet die zu fahrenden Segmente in
  // Ausfuehrungsreihenfolge inkl. Verfahrwege. EIGENSTAENDIGER Canvas fuer den
  // Preview-Reiter — der Design-Canvas bleibt unangetastet. Die Segmente kommen
  // aus dem Core (jobPreview); das Frontend zeichnet nur (CLAUDE.md Regel 1/2).
  import { onMount } from "svelte";
  import { jobPreview, type JobPreview, type Scene } from "./core";

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

  let preview = $state<JobPreview | null>(null);
  let loading = $state(false);
  // Verfahrwege (Leerfahrten) ein-/ausblendbar.
  let showTravel = $state(true);

  // Ansicht (wie Canvas.svelte).
  let zoom = $state(1.2);
  let panX = $state(40);
  let panY = $state(40);
  let viewTouched = false;

  const toScreen = (x: number, y: number): [number, number] => [x * zoom + panX, y * zoom + panY];
  const toMm = (px: number, py: number): [number, number] => [(px - panX) / zoom, (py - panY) / zoom];

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

  // Reihenfolge-Verlauf: fruehe Segmente kuehl (blau), spaete warm (magenta).
  // t in 0..1 entlang seq. Ergibt einen gut lesbaren Start→Ende-Verlauf.
  function seqColor(t: number): string {
    const h = 210 - 210 * t; // 210° (blau) → 0° (rot)
    return `hsl(${h}, 85%, 60%)`;
  }

  async function reload() {
    loading = true;
    try {
      preview = await jobPreview();
    } finally {
      loading = false;
    }
    draw();
  }

  function draw() {
    if (rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      render();
    });
  }

  function render() {
    if (!canvasEl) return;
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;
    const w = canvasEl.width, h = canvasEl.height;
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = "#141518";
    ctx.fillRect(0, 0, w, h);
    drawGrid(ctx, w, h);
    drawBed(ctx);
    drawMoves(ctx);
  }

  function drawGrid(ctx: CanvasRenderingContext2D, w: number, h: number) {
    let step = 50;
    while (step * zoom < 8) step *= 2;
    const [tlx, tly] = toMm(0, 0);
    const [brx, bry] = toMm(w, h);
    ctx.lineWidth = 1;
    ctx.strokeStyle = "rgba(255,255,255,0.06)";
    ctx.beginPath();
    for (let x = Math.floor(tlx / step) * step; x <= brx; x += step) {
      const sx = toScreen(x, 0)[0];
      ctx.moveTo(sx, 0); ctx.lineTo(sx, h);
    }
    for (let y = Math.floor(tly / step) * step; y <= bry; y += step) {
      const sy = toScreen(0, y)[1];
      ctx.moveTo(0, sy); ctx.lineTo(w, sy);
    }
    ctx.stroke();
  }

  function drawBed(ctx: CanvasRenderingContext2D) {
    const [x0, y0] = toScreen(0, 0);
    ctx.strokeStyle = "rgba(90,150,220,0.9)";
    ctx.lineWidth = 1.5;
    ctx.strokeRect(x0, y0, bedW * zoom, bedH * zoom);
    ctx.strokeStyle = "rgb(240,180,60)";
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.moveTo(x0, y0); ctx.lineTo(x0 + 18, y0);
    ctx.moveTo(x0, y0); ctx.lineTo(x0, y0 + 18);
    ctx.stroke();
  }

  function drawMoves(ctx: CanvasRenderingContext2D) {
    if (!preview || preview.moves.length === 0) return;
    const n = preview.moves.length;

    // Verfahrwege zuerst (blass, gestrichelt), damit die Arbeit obenauf liegt.
    if (showTravel) {
      ctx.setLineDash([4, 4]);
      ctx.strokeStyle = "rgba(255,255,255,0.22)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (const m of preview.moves) {
        if (m.kind !== "Travel") continue;
        const [ax, ay] = toScreen(m.from[0], m.from[1]);
        const [bx, by] = toScreen(m.to[0], m.to[1]);
        ctx.moveTo(ax, ay); ctx.lineTo(bx, by);
      }
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // Arbeitssegmente mit Reihenfolge-Verlauf (Start kuehl → Ende warm).
    ctx.lineWidth = 1.8;
    ctx.lineCap = "round";
    for (const m of preview.moves) {
      if (m.kind === "Travel") continue;
      const t = n > 1 ? m.seq / (n - 1) : 0;
      ctx.strokeStyle = seqColor(t);
      const [ax, ay] = toScreen(m.from[0], m.from[1]);
      const [bx, by] = toScreen(m.to[0], m.to[1]);
      ctx.beginPath();
      ctx.moveTo(ax, ay); ctx.lineTo(bx, by);
      ctx.stroke();
    }

    // Start-Marker (gruen) und End-Marker (rot) fuer die Fahrtrichtung.
    const work = preview.moves.filter((m) => m.kind !== "Travel");
    if (work.length > 0) {
      const first = work[0], last = work[work.length - 1];
      const [sx, sy] = toScreen(first.from[0], first.from[1]);
      const [ex, ey] = toScreen(last.to[0], last.to[1]);
      ctx.fillStyle = "#3fb27f";
      ctx.beginPath(); ctx.arc(sx, sy, 5, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = "#ff5c62";
      ctx.beginPath(); ctx.arc(ex, ey, 5, 0, Math.PI * 2); ctx.fill();
    }
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
    resize();
    const ro = new ResizeObserver(resize);
    if (wrapEl) ro.observe(wrapEl);
    return () => { ro.disconnect(); if (rafId) cancelAnimationFrame(rafId); };
  });

  // Bei Sichtbarwerden / Zustandsaenderung neu vom Core laden.
  $effect(() => { scene; reload(); });
  // Redraw bei Ansichts-/Filteraenderung.
  $effect(() => { zoom; panX; panY; showTravel; preview; insets; draw(); });

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
    {:else if !preview || preview.moves.length === 0}
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
