<script lang="ts">
  import { rgb, type Scene, type Shape } from "./core";

  type Tool = "select" | "rect" | "ellipse";

  let {
    scene,
    tool,
    ondrawrect,
    ondrawellipse,
    onselectat,
    onselectrect,
    onmove,
    onscale,
  }: {
    scene: Scene;
    tool: Tool;
    ondrawrect: (x: number, y: number, w: number, h: number) => void;
    ondrawellipse: (cx: number, cy: number, rx: number, ry: number) => void;
    onselectat: (x: number, y: number, additive: boolean) => void;
    onselectrect: (x1: number, y1: number, x2: number, y2: number) => void;
    onmove: (dx: number, dy: number) => void;
    onscale: (
      start: [number, number, number, number],
      target: [number, number, number, number],
    ) => void;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let wrapEl: HTMLDivElement;

  // Ansicht.
  let zoom = $state(1.2);
  let panX = $state(40);
  let panY = $state(40);

  const HANDLE_PX = 8;

  const toScreen = (x: number, y: number): [number, number] => [x * zoom + panX, y * zoom + panY];
  const toMm = (px: number, py: number): [number, number] => [(px - panX) / zoom, (py - panY) / zoom];

  // ---- Interaktions-Zustand (lokal, nur während einer Geste) ----------------
  type Drag =
    | { kind: "draw"; sx: number; sy: number; cx: number; cy: number }
    | { kind: "marquee"; sx: number; sy: number; cx: number; cy: number }
    | { kind: "move"; sx: number; sy: number; dx: number; dy: number }
    | {
        kind: "scale";
        handle: HandleId;
        start: [number, number, number, number];
        cur: [number, number, number, number];
      }
    | { kind: "pan"; px: number; py: number; ox: number; oy: number }
    | null;
  let drag = $state<Drag>(null);

  type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

  // ---- BBox-Helfer ----------------------------------------------------------
  function shapeBBox(s: Shape): [number, number, number, number] {
    if ("Rect" in s.geo) {
      const { x, y, w, h } = s.geo.Rect;
      return [x, y, w, h];
    }
    if ("Ellipse" in s.geo) {
      const { cx, cy, rx, ry } = s.geo.Ellipse;
      return [cx - rx, cy - ry, rx * 2, ry * 2];
    }
    const { pts } = s.geo.Polyline;
    let a = Infinity, b = Infinity, c = -Infinity, d = -Infinity;
    for (const [px, py] of pts) {
      a = Math.min(a, px); b = Math.min(b, py); c = Math.max(c, px); d = Math.max(d, py);
    }
    return [a, b, c - a, d - b];
  }

  function selectionBBox(): [number, number, number, number] | null {
    if (!scene.selected.length) return null;
    let a = Infinity, b = Infinity, c = -Infinity, d = -Infinity;
    for (const idx of scene.selected) {
      const s = scene.shapes[idx];
      if (!s) continue;
      const [x, y, w, h] = shapeBBox(s);
      a = Math.min(a, x); b = Math.min(b, y); c = Math.max(c, x + w); d = Math.max(d, y + h);
    }
    if (a === Infinity) return null;
    return [a, b, c - a, d - b];
  }

  function handlePositions(box: [number, number, number, number]): [HandleId, number, number][] {
    const [x, y, w, h] = box;
    const cx = x + w / 2, cy = y + h / 2;
    return [
      ["nw", x, y], ["n", cx, y], ["ne", x + w, y],
      ["e", x + w, cy], ["se", x + w, y + h], ["s", cx, y + h],
      ["sw", x, y + h], ["w", x, cy],
    ];
  }

  function hitHandle(px: number, py: number): HandleId | null {
    if (scene.selected.length !== 1) return null;
    const box = selectionBBox();
    if (!box) return null;
    for (const [id, hx, hy] of handlePositions(box)) {
      const [sx, sy] = toScreen(hx, hy);
      if (Math.abs(px - sx) <= HANDLE_PX && Math.abs(py - sy) <= HANDLE_PX) return id;
    }
    return null;
  }

  function resizeBox(
    start: [number, number, number, number],
    handle: HandleId,
    dx: number,
    dy: number,
  ): [number, number, number, number] {
    let [x, y, w, h] = start;
    const left = handle === "w" || handle === "nw" || handle === "sw";
    const right = handle === "e" || handle === "ne" || handle === "se";
    const top = handle === "n" || handle === "nw" || handle === "ne";
    const bottom = handle === "s" || handle === "sw" || handle === "se";
    if (left) { x += dx; w -= dx; }
    if (right) { w += dx; }
    if (top) { y += dy; h -= dy; }
    if (bottom) { h += dy; }
    if (w < 0.1) w = 0.1;
    if (h < 0.1) h = 0.1;
    return [x, y, w, h];
  }

  // ---- Zeichnen -------------------------------------------------------------
  function draw() {
    if (!canvasEl) return;
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;
    const w = canvasEl.width, h = canvasEl.height;
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = "#141518";
    ctx.fillRect(0, 0, w, h);
    drawGrid(ctx, w, h);
    drawBed(ctx);
    for (const s of scene.shapes) drawShape(ctx, s);
    drawSelection(ctx);
    drawGesturePreview(ctx);
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
    const bw = scene.bed_w_mm * zoom, bh = scene.bed_h_mm * zoom;
    ctx.fillStyle = "rgba(90,150,220,0.10)";
    ctx.fillRect(x0, y0, bw, bh);
    ctx.strokeStyle = "rgba(90,150,220,0.9)";
    ctx.lineWidth = 1.5;
    ctx.strokeRect(x0, y0, bw, bh);
    ctx.strokeStyle = "rgb(240,180,60)";
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.moveTo(x0, y0); ctx.lineTo(x0 + 18, y0);
    ctx.moveTo(x0, y0); ctx.lineTo(x0, y0 + 18);
    ctx.stroke();
  }

  function layerColor(s: Shape): string {
    const l = scene.layers[s.layer_id];
    return l ? rgb(l.color) : "#ff5c62";
  }
  function layerFilled(s: Shape): boolean {
    const l = scene.layers[s.layer_id];
    return !!l && (l.mode === "Fill" || l.mode === "Raster");
  }

  // Live-Verschiebe-/Skalier-Offset einer Form beim Ziehen (nur visuell).
  function shapeDrawBox(s: Shape, idx: number): [number, number, number, number] {
    let box = shapeBBox(s);
    if (drag?.kind === "move" && scene.selected.includes(idx)) {
      box = [box[0] + drag.dx, box[1] + drag.dy, box[2], box[3]];
    } else if (drag?.kind === "scale" && scene.selected.includes(idx)) {
      const [sx, sy, sw, sh] = drag.start;
      const [tx, ty, tw, th] = drag.cur;
      const fx = sw > 0 ? tw / sw : 1, fy = sh > 0 ? th / sh : 1;
      const rx = sw > 0 ? (box[0] - sx) / sw : 0, ry = sh > 0 ? (box[1] - sy) / sh : 0;
      box = [tx + rx * tw, ty + ry * th, box[2] * fx, box[3] * fy];
    }
    return box;
  }

  function drawShape(ctx: CanvasRenderingContext2D, s: Shape) {
    const idx = scene.shapes.indexOf(s);
    const color = layerColor(s);
    const [bx, by, bw, bh] = shapeDrawBox(s, idx);
    ctx.save();
    if (s.rotation) {
      const [scx, scy] = toScreen(bx + bw / 2, by + bh / 2);
      ctx.translate(scx, scy);
      ctx.rotate((s.rotation * Math.PI) / 180);
      ctx.translate(-scx, -scy);
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    const [sx, sy] = toScreen(bx, by);
    if ("Ellipse" in s.geo) {
      ctx.ellipse(sx + (bw * zoom) / 2, sy + (bh * zoom) / 2, (bw * zoom) / 2, (bh * zoom) / 2, 0, 0, Math.PI * 2);
    } else if ("Polyline" in s.geo) {
      // Polyline: Punkte mit Live-Offset (nur move unterstützt genau; sonst BBox-Rahmen)
      const { pts, closed } = s.geo.Polyline;
      const offx = drag?.kind === "move" && scene.selected.includes(idx) ? drag.dx : 0;
      const offy = drag?.kind === "move" && scene.selected.includes(idx) ? drag.dy : 0;
      pts.forEach((p, i) => {
        const [px, py] = toScreen(p[0] + offx, p[1] + offy);
        if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
      });
      if (closed) ctx.closePath();
    } else {
      ctx.rect(sx, sy, bw * zoom, bh * zoom);
    }
    if (layerFilled(s)) { ctx.fillStyle = color + "48"; ctx.fill(); }
    ctx.stroke();
    ctx.restore();
  }

  function drawSelection(ctx: CanvasRenderingContext2D) {
    if (!scene.selected.length) return;
    ctx.strokeStyle = "#4c82f7";
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 3]);
    for (const idx of scene.selected) {
      const s = scene.shapes[idx];
      if (!s) continue;
      const [bx, by, bw, bh] = shapeDrawBox(s, idx);
      const [x, y] = toScreen(bx, by);
      ctx.strokeRect(x - 3, y - 3, bw * zoom + 6, bh * zoom + 6);
    }
    ctx.setLineDash([]);
    // Handles bei genau einem Objekt.
    if (scene.selected.length === 1) {
      const s = scene.shapes[scene.selected[0]];
      if (s && !s.rotation) {
        const box = shapeDrawBox(s, scene.selected[0]);
        ctx.fillStyle = "#fff";
        ctx.strokeStyle = "#4c82f7";
        for (const [, hx, hy] of handlePositions(box)) {
          const [px, py] = toScreen(hx, hy);
          ctx.fillRect(px - 4, py - 4, 8, 8);
          ctx.strokeRect(px - 4, py - 4, 8, 8);
        }
      }
    }
  }

  function drawGesturePreview(ctx: CanvasRenderingContext2D) {
    if (drag?.kind === "draw") {
      const x = Math.min(drag.sx, drag.cx), y = Math.min(drag.sy, drag.cy);
      const w = Math.abs(drag.cx - drag.sx), h = Math.abs(drag.cy - drag.sy);
      const [px, py] = toScreen(x, y);
      ctx.strokeStyle = "rgba(255,255,255,0.6)";
      ctx.setLineDash([5, 4]);
      ctx.lineWidth = 1.2;
      if (tool === "ellipse") {
        ctx.beginPath();
        ctx.ellipse(px + (w * zoom) / 2, py + (h * zoom) / 2, (w * zoom) / 2, (h * zoom) / 2, 0, 0, Math.PI * 2);
        ctx.stroke();
      } else {
        ctx.strokeRect(px, py, w * zoom, h * zoom);
      }
      ctx.setLineDash([]);
    } else if (drag?.kind === "marquee") {
      const x = Math.min(drag.sx, drag.cx), y = Math.min(drag.sy, drag.cy);
      const w = Math.abs(drag.cx - drag.sx), h = Math.abs(drag.cy - drag.sy);
      const [px, py] = toScreen(x, y);
      ctx.fillStyle = "rgba(90,150,220,0.12)";
      ctx.fillRect(px, py, w * zoom, h * zoom);
      ctx.strokeStyle = "#4c82f7";
      ctx.setLineDash([4, 3]);
      ctx.strokeRect(px, py, w * zoom, h * zoom);
      ctx.setLineDash([]);
    }
  }

  // ---- Maus -----------------------------------------------------------------
  function localXY(ev: MouseEvent): [number, number] {
    const r = canvasEl.getBoundingClientRect();
    return [ev.clientX - r.left, ev.clientY - r.top];
  }

  function onPointerDown(ev: PointerEvent) {
    canvasEl.setPointerCapture(ev.pointerId);
    const [px, py] = localXY(ev);
    // Mittel-Maus oder Space = Pan.
    if (ev.button === 1) {
      drag = { kind: "pan", px, py, ox: panX, oy: panY };
      return;
    }
    if (ev.button !== 0) return;
    const [mx, my] = toMm(px, py);

    if (tool === "rect" || tool === "ellipse") {
      drag = { kind: "draw", sx: mx, sy: my, cx: mx, cy: my };
      return;
    }
    // select-Werkzeug
    const h = hitHandle(px, py);
    if (h) {
      const box = selectionBBox()!;
      drag = { kind: "scale", handle: h, start: box, cur: box };
      return;
    }
    const additive = ev.shiftKey || ev.ctrlKey;
    // Auf einem selektierten Objekt? → Move. Sonst select + evtl. Move.
    const box = selectionBBox();
    const onSel =
      box &&
      mx >= box[0] && mx <= box[0] + box[2] &&
      my >= box[1] && my <= box[1] + box[3];
    if (onSel && !additive) {
      drag = { kind: "move", sx: mx, sy: my, dx: 0, dy: 0 };
      return;
    }
    // Erst selektieren (Command), dann ggf. Marquee wenn nichts getroffen.
    onselectat(mx, my, additive);
    // Marquee vorbereiten (falls ins Leere geklickt wurde, greift es beim Ziehen).
    drag = { kind: "marquee", sx: mx, sy: my, cx: mx, cy: my };
  }

  function onPointerMove(ev: PointerEvent) {
    const [px, py] = localXY(ev);
    const [mx, my] = toMm(px, py);
    if (!drag) return;
    if (drag.kind === "pan") {
      panX = drag.ox + (px - drag.px);
      panY = drag.oy + (py - drag.py);
    } else if (drag.kind === "draw" || drag.kind === "marquee") {
      drag = { ...drag, cx: mx, cy: my };
    } else if (drag.kind === "move") {
      drag = { ...drag, dx: mx - drag.sx, dy: my - drag.sy };
    } else if (drag.kind === "scale") {
      const dx = mx - (drag.start[0] + hxOffset(drag.handle, drag.start));
      const dy = my - (drag.start[1] + hyOffset(drag.handle, drag.start));
      drag = { ...drag, cur: resizeBox(drag.start, drag.handle, dx, dy) };
    }
    draw();
  }

  // Referenz-Kante des Handles in der Startbox (für konsistentes Delta).
  function hxOffset(h: HandleId, b: [number, number, number, number]): number {
    if (h === "e" || h === "ne" || h === "se") return b[2];
    if (h === "n" || h === "s") return b[2] / 2;
    return 0;
  }
  function hyOffset(h: HandleId, b: [number, number, number, number]): number {
    if (h === "s" || h === "sw" || h === "se") return b[3];
    if (h === "e" || h === "w") return b[3] / 2;
    return 0;
  }

  function onPointerUp(ev: PointerEvent) {
    if (!drag) return;
    const g = drag;
    drag = null;
    if (g.kind === "draw") {
      const x = Math.min(g.sx, g.cx), y = Math.min(g.sy, g.cy);
      const w = Math.abs(g.cx - g.sx), h = Math.abs(g.cy - g.sy);
      if (w > 1 && h > 1) {
        if (tool === "ellipse") ondrawellipse(x + w / 2, y + h / 2, w / 2, h / 2);
        else ondrawrect(x, y, w, h);
      }
    } else if (g.kind === "marquee") {
      const w = Math.abs(g.cx - g.sx), h = Math.abs(g.cy - g.sy);
      if (w > 0.5 || h > 0.5) onselectrect(g.sx, g.sy, g.cx, g.cy);
    } else if (g.kind === "move") {
      if (g.dx !== 0 || g.dy !== 0) onmove(g.dx, g.dy);
    } else if (g.kind === "scale") {
      onscale(g.start, g.cur);
    }
  }

  function onWheel(ev: WheelEvent) {
    ev.preventDefault();
    const [px, py] = localXY(ev);
    const [wx, wy] = toMm(px, py);
    zoom = Math.max(0.05, Math.min(40, zoom * (ev.deltaY < 0 ? 1.15 : 0.85)));
    panX = px - wx * zoom;
    panY = py - wy * zoom;
    draw();
  }

  function resize() {
    if (!wrapEl || !canvasEl) return;
    canvasEl.width = wrapEl.clientWidth;
    canvasEl.height = wrapEl.clientHeight;
    draw();
  }

  $effect(() => { scene; zoom; panX; panY; drag; draw(); });
  $effect(() => {
    resize();
    const ro = new ResizeObserver(resize);
    if (wrapEl) ro.observe(wrapEl);
    return () => ro.disconnect();
  });
</script>

<div class="wrap" bind:this={wrapEl}>
  <canvas
    bind:this={canvasEl}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onwheel={onWheel}
  ></canvas>
</div>

<style>
  .wrap { position: absolute; inset: 0; }
  canvas { display: block; touch-action: none; }
</style>
