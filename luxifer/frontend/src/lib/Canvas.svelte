<script lang="ts">
  import { onMount } from "svelte";
  import { rgb, polygonPreview, imageRender, shapeBBox, type Scene, type Shape, type ImageParams } from "./core";
  // WebGL-Geometrie-Ebene (ADR 0008): Konturen/Grid/Bett/Bilder auf der GPU.
  // Die Overlays (Handles, Lineale, Mess-/Node-Griffe) bleiben auf dem 2D-Canvas
  // DARÜBER — der trägt weiterhin alle Pointer-Handler.
  import { GlRenderer, type LineBatch, type GlBatch } from "./gl/renderer";
  import { type Camera } from "./gl/camera";
  import { shapesToBatch, type PointXf } from "./gl/design-render";

  type Tool = "select" | "rect" | "ellipse" | "line" | "polyline" | "polygon" | "spline" | "measure" | "bezier" | "node";

  let {
    scene,
    tool,
    activeShape,
    insets,
    active = true,
    fitTrigger = 0,
    ondrawrect,
    ondrawellipse,
    ondrawline,
    ondrawpolyline,
    ondrawpolygon,
    onselectat,
    onselectrect,
    onmove,
    onscale,
    oneditimage,
    onedittext,
    filletpick = false,
    filletsel = [],
    onfilletcorner,
    bridgepick = false,
    bridgewidth = 2,
    onbridgestroke,
    ondragnode,
    onhitnodesegment,
    onsplitnode,
    ondeletenode,
    ontogglenode,
    onbezierdone,
    laserHead,
    laserOrigin,
  }: {
    scene: Scene;
    tool: Tool;
    // Aktuell gewaehlte Polygon-Form (Katalog-`id`, z. B. "hex").
    activeShape: string;
    // Sichtbarer Arbeitsbereich: Beim Wechsel zurueck in Design wird neu eingepasst.
    active?: boolean;
    // Externer FitView-Impuls (Start/Tabwechsel). Der Wert selbst ist egal.
    fitTrigger?: number;
    // Freie Raender in Pixeln, in die das Bett beim Start eingepasst wird
    // (verdeckt von Header/Panels). Optional; Default 0.
    insets?: { top: number; right: number; bottom: number; left: number };
    ondrawrect: (x: number, y: number, w: number, h: number) => void;
    ondrawellipse: (cx: number, cy: number, rx: number, ry: number) => void;
    ondrawline: (x1: number, y1: number, x2: number, y2: number) => void;
    ondrawpolyline: (pts: [number, number][], closed: boolean) => void;
    // Polygon: Form-`id`, Zentrum, Aussenradius, Drehung (Grad).
    ondrawpolygon: (shape: string, cx: number, cy: number, r: number, rot: number) => void;
    onselectat: (x: number, y: number, additive: boolean) => void;
    onselectrect: (x1: number, y1: number, x2: number, y2: number) => void;
    // Geben ein Promise zurueck, damit der Canvas die Live-Vorschau erst nach
    // dem Core-Update loesen kann (verhindert das Aufblitzen an der alten Stelle).
    onmove: (dx: number, dy: number) => void | Promise<void>;
    onscale: (
      start: [number, number, number, number],
      target: [number, number, number, number],
    ) => void | Promise<void>;
    // Doppelklick auf ein Bild-Shape: Editor oeffnen (Shape-Index).
    oneditimage?: (index: number) => void;
    onedittext?: (index: number) => void;
    // Ecken-Pick-Modus (Fillet): Ecken werden markiert und sind klickbar.
    filletpick?: boolean;
    // Gewählte Ecken als "shapeIdx:cornerIdx".
    filletsel?: string[];
    onfilletcorner?: (shape: number, corner: number) => void;
    // Haltesteg-Modus: Klick auf eine Kontur meldet die mm-Position.
    bridgepick?: boolean;
    bridgewidth?: number;
    onbridgestroke?: (x0: number, y0: number, x1: number, y1: number) => void;
    // Node-Editor: Knoten/Handle ziehen, Segment teilen, Knoten löschen.
    ondragnode?: (shape: number, node: number, part: "anchor" | "in" | "out", x: number, y: number, begin: boolean) => void;
    onhitnodesegment?: (x: number, y: number, tolerance: number) => Promise<{ shape: number; segment: number; t: number } | null>;
    onsplitnode?: (shape: number, segStart: number, t: number) => void;
    ondeletenode?: (shape: number, node: number) => void;
    ontogglenode?: (shape: number, node: number) => void;
    // Fertiger Bézier-Pfad: Knoten (Anker+Tangenten in mm) + geschlossen.
    onbezierdone?: (nodes: { p: [number, number]; h_in: [number, number] | null; h_out: [number, number] | null }[], closed: boolean) => void;
    // Laser-Positionen (mm) fuer Marker: Kopf und Benutzerursprung. Optional.
    laserHead?: [number, number] | null;
    laserOrigin?: [number, number] | null;
  } = $props();

  let canvasEl: HTMLCanvasElement;   // oberer 2D-Layer: Overlays + Pointer
  let glCanvasEl: HTMLCanvasElement;  // unterer WebGL-Layer: Geometrie
  let wrapEl: HTMLDivElement;

  // ---- WebGL-Geometrie-Ebene (ADR 0008) -------------------------------------
  // Der Renderer und die hochgeladenen GPU-Batches. Konturen werden bei
  // Datenänderung EINMAL hochgeladen (rebuildGeo), bei Pan/Zoom ändert sich nur
  // die Kamera-Matrix — kein Neu-Upload. Bilder liegen als Texturen an ihrer Box.
  let gl: GlRenderer | null = null;
  let shapeBatch: GlBatch | null = null;

  // ---- Bild-Cache (ADR 0004 §3a) --------------------------------------------
  // Gerenderte Bild-Bitmaps (asset+params → HTMLImageElement). Move/Resize
  // zeichnen nur `drawImage` mit neuen Koordinaten neu — KEIN Neurendern. Ein
  // Eintrag wird nur einmal erzeugt (bzw. wenn sich die Editor-Parameter aendern).
  const imgCache = new Map<string, HTMLImageElement>();
  // Welche Keys werden gerade geladen (verhindert Doppel-Requests pro Frame).
  const imgLoading = new Set<string>();

  // WICHTIG (ADR 0004 §3, Invariante): Das Canvas zeigt das Bild UNVERAENDERT —
  // die einzige Ausnahme ist `invert_editor`. Schwelle/Helligkeit/Kontrast/Gamma
  // wirken NUR in der Editor-Vorschau und beim spaeteren Rastern, NICHT im Canvas.
  // Deshalb rendert das Canvas immer neutral (Graustufe, keine Tonwertaenderung)
  // und wendet nur invert_editor an. Cache-Key = asset + invert_editor.
  function imgKey(asset: string, p: ImageParams): string {
    return `${asset}|${p.invert_editor ? 1 : 0}`;
  }

  // Liefert das gecachte Bitmap oder startet asynchron das Rendern und loest
  // danach genau einen Redraw aus. Gibt `null`, solange noch nicht geladen.
  function cachedImage(asset: string, p: ImageParams): HTMLImageElement | null {
    const key = imgKey(asset, p);
    const hit = imgCache.get(key);
    if (hit) return hit;
    if (!imgLoading.has(key)) {
      imgLoading.add(key);
      // Neutrale Params: rohes Graustufen-Asset, nur invert_editor greift.
      const neutral: ImageParams = {
        mode: "Grayscale",
        threshold: 128,
        brightness: 0,
        contrast: 0,
        gamma: 1.0,
        invert_editor: p.invert_editor,
        invert_laser: false,
      };
      imageRender(asset, neutral, p.invert_editor).then((url) => {
        imgLoading.delete(key);
        if (!url) return;
        const el = new Image();
        el.onload = () => {
          imgCache.set(key, el);
          draw();
        };
        el.src = url;
      });
    }
    return null;
  }

  // Ansicht.
  let zoom = $state(1.2);
  let panX = $state(40);
  let panY = $state(40);
  // Solange der Nutzer die Ansicht noch nicht selbst bewegt hat (Pan/Zoom),
  // passt sich das Bett automatisch in den freien Bereich ein.
  let viewTouched = false;
  let fitRaf = 0;
  let fitSeq = 0;

  function clearScheduledFit() {
    if (fitRaf) cancelAnimationFrame(fitRaf);
    fitRaf = 0;
  }

  function scheduleFitBed(resetTouched = false) {
    clearScheduledFit();
    const seq = ++fitSeq;
    let frames = 4;
    if (resetTouched) viewTouched = false;
    const tick = () => {
      fitRaf = 0;
      if (seq !== fitSeq) return;
      resize();
      if (!viewTouched) {
        fitBed();
        draw();
      }
      frames -= 1;
      if (frames > 0 && !viewTouched) fitRaf = requestAnimationFrame(tick);
    };
    fitRaf = requestAnimationFrame(tick);
  }

  // Passt das Bett zentriert in den freien Bereich (Canvas minus Insets) ein.
  function fitBed() {
    if (!canvasEl) return;
    const cw = canvasEl.width, ch = canvasEl.height;
    const rawIns = insets ?? { top: 0, right: 0, bottom: 0, left: 0 };
    const ins = {
      ...rawIns,
      top: rawIns.top + RULER_PX,
      left: rawIns.left + RULER_PX,
    };
    const availW = Math.max(50, cw - ins.left - ins.right);
    const availH = Math.max(50, ch - ins.top - ins.bottom);
    const bw = scene.bed_w_mm, bh = scene.bed_h_mm;
    if (bw <= 0 || bh <= 0) return;
    const margin = 0.9; // etwas Luft rundherum
    const nz = Math.min(availW / bw, availH / bh) * margin;
    // Bett-Mitte auf die Mitte des freien Bereichs legen.
    const freeCx = ins.left + availW / 2;
    const freeCy = ins.top + availH / 2;
    const nx = freeCx - (bw / 2) * nz;
    const ny = freeCy - (bh / 2) * nz;
    // Nur schreiben, wenn sich wirklich etwas aendert. Sonst triggert jede
    // Aktion (neue Scene → neues insets-Objekt) den Effect, fitBed setzt
    // identische Werte neu und loest eine ueberfluessige Redraw-Kaskade aus.
    if (
      Math.abs(nz - zoom) > 1e-6 ||
      Math.abs(nx - panX) > 1e-3 ||
      Math.abs(ny - panY) > 1e-3
    ) {
      zoom = nz;
      panX = nx;
      panY = ny;
    }
  }

  const HANDLE_PX = 8;

  const toScreen = (x: number, y: number): [number, number] => [x * zoom + panX, y * zoom + panY];
  const toMm = (px: number, py: number): [number, number] => [(px - panX) / zoom, (py - panY) / zoom];
  // Kamera für die WebGL-Ebene — identische Konvention (px = x*zoom + pan).
  const cam = (): Camera => ({ zoom, panX, panY });

  // ---- Interaktions-Zustand (lokal, nur während einer Geste) ----------------
  type Drag =
    | { kind: "measure" }
    | { kind: "node"; shape: number; node: number; part: "anchor" | "in" | "out"; began: boolean }
    | { kind: "bridge"; sx: number; sy: number; cx: number; cy: number }
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

  // ---- Polylinien-Modus (mehrere Klicks; lebt ueber die einzelne Geste) ------
  // Gesetzte Stuetzpunkte in mm; `polyCursor` ist die aktuelle Mausposition
  // fuers Gummiband. Beides ist fluechtiger UI-Zustand, kein Wahrheitszustand:
  // erst der Abschluss schickt EINE fertige Polylinie an den Core.
  let polyPts = $state<[number, number][]>([]);
  let polyCursor = $state<[number, number] | null>(null);
  // Cursor nah genug am ersten Punkt, um die Kontur zu schliessen? (>= 3 Punkte)
  let polyNearStart = $state(false);

  // Bézier-Feder (Inkscape-Stil): Knoten mit Anker + Tangenten, live gezeichnet.
  // Klick = Ecke, Klick+Ziehen = glatter Knoten (hOut folgt, hIn = Spiegel).
  type BNode = { p: [number, number]; hIn: [number, number] | null; hOut: [number, number] | null };
  let bez = $state<{ nodes: BNode[]; cursor: [number, number] | null; closed: boolean } | null>(null);
  // Index statt Objektreferenz: Svelte proxifiziert Elemente eines $state-
  // Arrays. Eine vorher gemerkte Roh-Referenz würde außerhalb des Arrays
  // verändert; die sichtbare und später gespeicherte Kurve bliebe gerade.
  let bezDrag = $state<number | null>(null);
  let bezDragStart: [number, number] | null = null;
  let bezDragged = false;
  // Fangradius um den ersten Punkt in Bildschirm-Pixeln.
  const POLY_CLOSE_PX = 10;

  // Liegt die Bildschirmposition (px,py) im Fangradius des ersten Punkts?
  function nearFirstPoint(px: number, py: number): boolean {
    if (polyPts.length < 3) return false;
    const [fx, fy] = toScreen(...polyPts[0]);
    return Math.hypot(px - fx, py - fy) <= POLY_CLOSE_PX;
  }

  type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

  // ---- BBox-Helfer ----------------------------------------------------------
  function selectionBBox(): [number, number, number, number] | null {
    return scene.selection_bbox ?? null;
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
    const corner = (left || right) && (top || bottom);
    if (corner) {
      // Eck-Handle: Seitenverhaeltnis wahren. Ein gemeinsamer Faktor aus dem
      // groesseren relativen Delta, verankert an der gegenueberliegenden Ecke.
      const sw0 = start[2], sh0 = start[3];
      const fx = 1 + (right ? dx : -dx) / sw0;
      const fy = 1 + (bottom ? dy : -dy) / sh0;
      const f = Math.max(0.001, Math.max(fx, fy)); // dominante Achse fuehrt
      w = sw0 * f;
      h = sh0 * f;
      // Gegenueberliegende Ecke fix halten.
      if (left) x = start[0] + start[2] - w;
      if (top) y = start[1] + start[3] - h;
    } else {
      if (left) { x += dx; w -= dx; }
      if (right) { w += dx; }
      if (top) { y += dy; h -= dy; }
      if (bottom) { h += dy; }
    }
    if (w < 0.1) w = 0.1;
    if (h < 0.1) h = 0.1;
    return [x, y, w, h];
  }

  // ---- Zeichnen -------------------------------------------------------------
  // Gedrosselt ueber requestAnimationFrame: Egal wie oft `draw()` pro Frame
  // gerufen wird (mehrere Effects + Event-Handler), es wird nur EINMAL pro
  // Frame tatsaechlich neu gezeichnet. Das war die Hauptquelle des Ruckelns —
  // eine Mausbewegung loeste vorher 3–4 synchrone Full-Redraws aus.
  let rafId = 0;
  function draw() {
    if (rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      renderNow();
    });
  }

  // Während einer Pointer-Geste synchron zeichnen (kein rAF-Hop). Seit der
  // WebGL-Umstellung ist ein Redraw billig, ein eingeplanter rAF wird entwertet.
  function drawSync() {
    if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
    renderNow();
  }

  function renderNow() {
    if (!canvasEl) return;
    renderGeo();      // untere WebGL-Ebene: Konturen, Grid, Bett, Bilder
    renderOverlay();  // obere 2D-Ebene: Füllungen + alle Overlays (transparent)
  }

  // ---- WebGL-Ebene: Geometrie (ADR 0008) ------------------------------------
  // Der aktuelle Live-Drag-Transformer: während move/scale wandern selektierte
  // Konturen mit. Ist er gesetzt, wird der Shape-Batch pro Frame neu gebaut
  // (nur so lange die Geste läuft); sonst bleibt der einmal hochgeladene Batch.
  function liveXf(): PointXf | null {
    if (drag?.kind === "move") {
      const sel = new Set(scene.selected);
      const dx = drag.dx, dy = drag.dy;
      return (x, y, idx) => (sel.has(idx) ? [x + dx, y + dy] : [x, y]);
    }
    if (drag?.kind === "scale") {
      const sel = new Set(scene.selected);
      const [sx, sy, sw, sh] = drag.start;
      const [tx, ty, tw, th] = drag.cur;
      const fx = sw > 0 ? tw / sw : 1, fy = sh > 0 ? th / sh : 1;
      return (x, y, idx) => (sel.has(idx) ? [tx + (x - sx) * fx, ty + (y - sy) * fy] : [x, y]);
    }
    return null;
  }

  // Konturen-Batch einmal auf die GPU laden. Bei Live-Drag NICHT cachen — dann
  // baut renderGeo pro Frame neu (wenige Segmente wandern, Rest liegt statisch).
  function rebuildGeo() {
    if (!gl) return;
    if (shapeBatch) { gl.free(shapeBatch); shapeBatch = null; }
    const batch = shapesToBatch(scene);
    if (batch.positions.length) shapeBatch = gl.upload(batch.positions, batch.colors);
  }

  function renderGeo() {
    if (!glCanvasEl) return;
    if (!gl) { try { gl = new GlRenderer(glCanvasEl); } catch { return; } rebuildGeo(); }
    if (gl.isLost()) { try { gl = new GlRenderer(glCanvasEl); rebuildGeo(); } catch { return; } }
    const w = glCanvasEl.width, h = glCanvasEl.height;
    gl.begin(cam(), w, h, [0.078, 0.082, 0.094]); // #141518

    // Grid + Bett (wenige Linien, sichtbereichsabhängig) pro Frame bauen.
    const grid = gridBatchGl(w, h);
    const gb = gl.upload(grid.positions, grid.colors);
    gl.drawBatch(gb, "lines"); gl.free(gb);
    const bed = bedBatchGl();
    const bb = gl.upload(bed.positions, bed.colors);
    gl.drawBatch(bb, "lines"); gl.free(bb);

    // Konturen: bei Live-Drag pro Frame neu (Geste), sonst der Cache-Batch.
    // Bilder liegen NICHT hier — sie bleiben auf der 2D-Ebene (drawImage-Quad,
    // ohnehin billig; kein WebGL-Textur-Umweg nötig).
    const xf = liveXf();
    if (xf) {
      const b = shapesToBatch(scene, xf);
      if (b.positions.length) {
        const tmp = gl.upload(b.positions, b.colors);
        gl.drawBatch(tmp, "lines"); gl.free(tmp);
      }
    } else if (shapeBatch) {
      gl.drawBatch(shapeBatch, "lines");
    }
  }

  // Grid als Line-Batch in mm (Shader rechnet mm→Clip). Wie drawGrid, aber GPU.
  function gridBatchGl(w: number, h: number): LineBatch {
    let step = 50;
    while (step * zoom < 8) step *= 2;
    const [tlx, tly] = toMm(0, 0);
    const [brx, bry] = toMm(w, h);
    const pos: number[] = [];
    for (let x = Math.floor(tlx / step) * step; x <= brx; x += step) pos.push(x, tly, x, bry);
    for (let y = Math.floor(tly / step) * step; y <= bry; y += step) pos.push(tlx, y, brx, y);
    return solidBatch(pos, [1, 1, 1, 0.06]);
  }

  // Bett-Rechteck (blau) + Ursprungswinkel (gelb) als Line-Batch.
  function bedBatchGl(): LineBatch {
    const bw = scene.bed_w_mm, bh = scene.bed_h_mm;
    const pos: number[] = [], col: number[] = [];
    const line = (x0: number, y0: number, x1: number, y1: number, c: number[]) => {
      pos.push(x0, y0, x1, y1);
      col.push(c[0], c[1], c[2], c[3], c[0], c[1], c[2], c[3]);
    };
    const blue = [0.35, 0.59, 0.86, 0.9];
    line(0, 0, bw, 0, blue); line(bw, 0, bw, bh, blue);
    line(bw, bh, 0, bh, blue); line(0, bh, 0, 0, blue);
    // Ursprungswinkel: konstante Bildschirmlänge (18 px) in mm umgerechnet.
    const armMm = 18 / zoom, gold = [0.94, 0.71, 0.24, 1];
    line(0, 0, armMm, 0, gold); line(0, 0, 0, armMm, gold);
    return { positions: new Float32Array(pos), colors: new Float32Array(col) };
  }

  // Einfarbiger Line-Batch (eine Farbe für alle Vertices).
  function solidBatch(pos: number[], rgba: [number, number, number, number]): LineBatch {
    const n = pos.length / 2;
    const col = new Float32Array(n * 4);
    for (let i = 0; i < n; i++) col.set(rgba, i * 4);
    return { positions: new Float32Array(pos), colors: col };
  }

  // ---- 2D-Ebene: Füllungen + Overlays (transparent über der WebGL-Ebene) ----
  function renderOverlay() {
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;
    const w = canvasEl.width, h = canvasEl.height;
    ctx.clearRect(0, 0, w, h); // transparent — die WebGL-Ebene scheint durch
    // Füllungen gefüllter Layer (die Kontur selbst kommt aus WebGL).
    scene.shapes.forEach((s, i) => drawFill(ctx, s, i));
    drawSelection(ctx);
    drawGesturePreview(ctx);
    drawPolyPreview(ctx);
    drawBezierDraft(ctx);
    drawLaserMarkers(ctx);
    drawMeasure(ctx);
    drawBridgePreview(ctx);
    drawFilletMarkers(ctx);
    drawNodes(ctx);
    drawRulers(ctx, w, h);
  }

  // ---- Lineale (mm) oben + links ------------------------------------------
  const RULER_PX = 22;
  function drawRulers(ctx: CanvasRenderingContext2D, w: number, h: number) {
    const ins = insets ?? { top: 0, right: 0, bottom: 0, left: 0 };
    const rx = Math.max(0, ins.left);
    const ry = Math.max(0, ins.top);
    const rw = Math.max(0, w - rx - ins.right);
    const rh = Math.max(0, h - ry - ins.bottom);
    if (rw <= RULER_PX || rh <= RULER_PX) return;

    // Tick-Schritt wie das Grid an den Zoom anpassen.
    let step = 1;
    while (step * zoom < 40) step *= step % 3 === 2 ? 2.5 : 2; // 1,2,5,10,…
    step = Math.round(step * 100) / 100;

    ctx.fillStyle = "rgba(20, 21, 24, 0.92)";
    ctx.fillRect(rx, ry, rw, RULER_PX);
    ctx.fillRect(rx, ry, RULER_PX, rh);
    ctx.strokeStyle = "rgba(255,255,255,0.15)";
    ctx.lineWidth = 1;
    ctx.strokeRect(rx - 1, ry - 1, rw + 2, RULER_PX + 1);
    ctx.strokeRect(rx - 1, ry - 1, RULER_PX + 1, rh + 2);

    ctx.fillStyle = "rgba(255,255,255,0.55)";
    ctx.font = "9px system-ui";
    ctx.strokeStyle = "rgba(255,255,255,0.4)";
    const [x0mm] = toMm(rx + RULER_PX, ry);
    const [x1mm] = toMm(rx + rw, ry);
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    for (let x = Math.floor(x0mm / step) * step; x <= x1mm; x += step) {
      const sx = toScreen(x, 0)[0];
      if (sx < rx + RULER_PX || sx > rx + rw) continue;
      ctx.beginPath();
      ctx.moveTo(sx, ry + RULER_PX - 6);
      ctx.lineTo(sx, ry + RULER_PX);
      ctx.stroke();
      ctx.fillText(String(Math.round(x)), sx + 2, ry + 2);
    }
    const [, y0mm] = toMm(rx, ry + RULER_PX);
    const [, y1mm] = toMm(rx, ry + rh);
    for (let y = Math.floor(y0mm / step) * step; y <= y1mm; y += step) {
      const sy = toScreen(0, y)[1];
      if (sy < ry + RULER_PX || sy > ry + rh) continue;
      ctx.beginPath();
      ctx.moveTo(rx + RULER_PX - 6, sy);
      ctx.lineTo(rx + RULER_PX, sy);
      ctx.stroke();
      ctx.save();
      ctx.translate(rx + 2, sy + 2);
      ctx.rotate(-Math.PI / 2);
      ctx.textAlign = "right";
      ctx.fillText(String(Math.round(y)), 0, 0);
      ctx.restore();
    }
    // Ecke abdecken.
    ctx.fillStyle = "rgba(20, 21, 24, 1)";
    ctx.fillRect(rx, ry, RULER_PX, RULER_PX);
  }

  // ---- Bézier-Feder: live gezeichnete Kurve + Tangenten während des Zeichnens
  // Lokale Kubik-Flatten nur für die VORSCHAU (Wahrheit erzeugt der Core).
  function cubic(p0: [number, number], p1: [number, number], p2: [number, number], p3: [number, number], out: [number, number][]) {
    const N = 16;
    for (let i = 1; i <= N; i++) {
      const t = i / N, u = 1 - t;
      out.push([
        u*u*u*p0[0] + 3*u*u*t*p1[0] + 3*u*t*t*p2[0] + t*t*t*p3[0],
        u*u*u*p0[1] + 3*u*u*t*p1[1] + 3*u*t*t*p2[1] + t*t*t*p3[1],
      ]);
    }
  }
  function bezFlatten(nodes: BNode[], closed: boolean): [number, number][] {
    if (nodes.length < 2) return nodes.map((n) => n.p);
    const out: [number, number][] = [nodes[0].p];
    const seg = (a: BNode, b: BNode) => cubic(a.p, a.hOut ?? a.p, b.hIn ?? b.p, b.p, out);
    for (let i = 0; i < nodes.length - 1; i++) seg(nodes[i], nodes[i + 1]);
    if (closed) seg(nodes[nodes.length - 1], nodes[0]);
    return out;
  }
  function drawBezierDraft(ctx: CanvasRenderingContext2D) {
    if (tool !== "bezier" || !bez || bez.nodes.length === 0) return;
    // Nur bereits gesetzte Knoten bilden die Kurve. Der Cursor ist ausdrücklich
    // KEIN vorläufiger Bézier-Knoten: Das Gummiband bleibt bis zum nächsten
    // Klick gerade. Erst Klick+Ziehen erzeugt Tangenten und damit eine Kurve.
    if (bez.nodes.length >= 2) {
      const flat = bezFlatten(bez.nodes, false);
      ctx.strokeStyle = "#ff5c62";
      ctx.lineWidth = 1.6;
      ctx.beginPath();
      flat.forEach((pt, i) => {
        const [x, y] = toScreen(pt[0], pt[1]);
        if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      });
      ctx.stroke();
    }
    if (bez.cursor && bezDrag === null) {
      const last = bez.nodes[bez.nodes.length - 1].p;
      const [lx, ly] = toScreen(last[0], last[1]);
      const [cx, cy] = toScreen(bez.cursor[0], bez.cursor[1]);
      ctx.strokeStyle = "#ff5c62";
      ctx.lineWidth = 1.4;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(lx, ly);
      ctx.lineTo(cx, cy);
      ctx.stroke();
      ctx.setLineDash([]);
    }
    // Tangenten des zuletzt/aktiv gezogenen Knotens + Anker-Quadrate.
    ctx.strokeStyle = "rgba(120,160,230,0.85)";
    ctx.lineWidth = 1;
    for (const nd of bez.nodes) {
      const [ax, ay] = toScreen(nd.p[0], nd.p[1]);
      for (const h of [nd.hIn, nd.hOut]) {
        if (!h) continue;
        const [hx, hy] = toScreen(h[0], h[1]);
        ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(hx, hy); ctx.stroke();
        ctx.beginPath(); ctx.arc(hx, hy, 3, 0, Math.PI * 2); ctx.fillStyle = "#7aa8ff"; ctx.fill();
      }
      ctx.fillStyle = "#fff";
      ctx.strokeStyle = "#4c82f7";
      ctx.lineWidth = 1.5;
      ctx.fillRect(ax - 3, ay - 3, 6, 6);
      ctx.strokeRect(ax - 3, ay - 3, 6, 6);
    }
  }

  function bezCommit() {
    if (bez && bez.nodes.length >= 2) {
      onbezierdone?.(bez.nodes.map((n) => ({ p: n.p, h_in: n.hIn, h_out: n.hOut })), bez.closed);
    }
    bez = null;
    bezDrag = null;
    draw();
  }

  // ---- Node-Editor: Knoten + Tangenten der selektierten Shapes ----------------
  const NODE_ACCENT = "#4c82f7";
  // Editier-Knoten einer Shape: aus dem Bézier-Meta ODER (Fallback) aus den
  // Konturpunkten der Polyline/Rect. So ist JEDE Vektor-Shape node-editierbar.
  function editNodes(s: Shape): { p: [number, number]; hIn: [number, number] | null; hOut: [number, number] | null }[] {
    if (s.bezier) return s.bezier.nodes.map((n) => ({ p: n.p, hIn: n.h_in ?? null, hOut: n.h_out ?? null }));
    if ("Polyline" in s.geo) return s.geo.Polyline.pts.map((p) => ({ p: [p[0], p[1]] as [number, number], hIn: null, hOut: null }));
    if ("Rect" in s.geo) {
      const { x, y, w, h } = s.geo.Rect;
      return [[x, y], [x + w, y], [x + w, y + h], [x, y + h]].map((p) => ({ p: p as [number, number], hIn: null, hOut: null }));
    }
    return [];
  }
  function drawNodes(ctx: CanvasRenderingContext2D) {
    if (tool !== "node") return;
    for (const idx of scene.selected) {
      const s = scene.shapes[idx];
      if (!s) continue;
      const nodes = editNodes(s);
      if (nodes.length === 0) continue;
      const bp = { nodes };
      // Tangenten-Linien zuerst (unter den Quadraten).
      ctx.strokeStyle = "rgba(120,160,230,0.8)";
      ctx.lineWidth = 1;
      for (const nd of bp.nodes) {
        const [ax, ay] = toScreen(nd.p[0], nd.p[1]);
        for (const h of [nd.hIn, nd.hOut]) {
          if (!h) continue;
          const [hx, hy] = toScreen(h[0], h[1]);
          ctx.beginPath();
          ctx.moveTo(ax, ay);
          ctx.lineTo(hx, hy);
          ctx.stroke();
          ctx.beginPath();
          ctx.arc(hx, hy, 3.5, 0, Math.PI * 2);
          ctx.fillStyle = "#7aa8ff";
          ctx.fill();
        }
      }
      // Anker-Quadrate (erster Knoten rot, wie v3).
      bp.nodes.forEach((nd, i) => {
        const [ax, ay] = toScreen(nd.p[0], nd.p[1]);
        ctx.fillStyle = "#ffffff";
        ctx.strokeStyle = i === 0 ? "#ff5c62" : NODE_ACCENT;
        ctx.lineWidth = 1.5;
        ctx.fillRect(ax - 3.5, ay - 3.5, 7, 7);
        ctx.strokeRect(ax - 3.5, ay - 3.5, 7, 7);
      });
    }
  }

  // Trifft ein Bildschirmpunkt einen Knoten/Handle? Liefert {shape,node,part}.
  function hitNode(px: number, py: number): { shape: number; node: number; part: "anchor" | "in" | "out" } | null {
    const tol = 7;
    for (const idx of scene.selected) {
      const s = scene.shapes[idx];
      if (!s) continue;
      const nodes = editNodes(s);
      for (let n = nodes.length - 1; n >= 0; n--) {
        const nd = nodes[n];
        // Handles zuerst (liegen "über" dem Anker beim Greifen).
        for (const [part, h] of [["in", nd.hIn], ["out", nd.hOut]] as const) {
          if (!h) continue;
          const [hx, hy] = toScreen(h[0], h[1]);
          if (Math.hypot(hx - px, hy - py) <= tol) return { shape: idx, node: n, part };
        }
        const [ax, ay] = toScreen(nd.p[0], nd.p[1]);
        if (Math.hypot(ax - px, ay - py) <= tol) return { shape: idx, node: n, part: "anchor" };
      }
    }
    return null;
  }

  // ---- Messen-Werkzeug (reine Anzeige, keine Wahrheit) ----------------------
  let measureA = $state<[number, number] | null>(null);
  let measureB = $state<[number, number] | null>(null);
  // Messung verschwindet beim Werkzeugwechsel.
  $effect(() => {
    if (tool !== "measure" && (measureA || measureB)) {
      measureA = null;
      measureB = null;
      draw();
    }
  });
  function drawBridgePreview(ctx: CanvasRenderingContext2D) {
    if (!drag || drag.kind !== "bridge") return;
    const [ax, ay] = toScreen(drag.sx, drag.sy);
    const [bx, by] = toScreen(drag.cx, drag.cy);
    const dx = bx - ax, dy = by - ay;
    const len = Math.hypot(dx, dy) || 1;
    // Band-Breite in Bildschirmpixeln (Breite in mm × zoom).
    const halfW = (bridgewidth / 2) * zoom;
    const nx = (-dy / len) * halfW, ny = (dx / len) * halfW;
    ctx.save();
    ctx.fillStyle = "rgba(14,165,233,0.15)";
    ctx.strokeStyle = "rgba(14,165,233,0.85)";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(ax - nx, ay - ny);
    ctx.lineTo(bx - nx, by - ny);
    ctx.lineTo(bx + nx, by + ny);
    ctx.lineTo(ax + nx, ay + ny);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    ctx.setLineDash([4, 4]);
    ctx.strokeStyle = "rgba(14,165,233,0.5)";
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.stroke();
    ctx.restore();
  }

  function drawMeasure(ctx: CanvasRenderingContext2D) {
    if (!measureA || !measureB) return;
    const [ax, ay] = toScreen(measureA[0], measureA[1]);
    const [bx, by] = toScreen(measureB[0], measureB[1]);
    ctx.strokeStyle = "#f0a500";
    ctx.lineWidth = 1.4;
    ctx.setLineDash([5, 4]);
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.stroke();
    ctx.setLineDash([]);
    for (const [px, py] of [[ax, ay], [bx, by]]) {
      ctx.beginPath();
      ctx.arc(px, py, 3, 0, Math.PI * 2);
      ctx.fillStyle = "#f0a500";
      ctx.fill();
    }
    const dx = measureB[0] - measureA[0];
    const dy = measureB[1] - measureA[1];
    const d = Math.hypot(dx, dy);
    const label = `${d.toFixed(2)} mm  (Δx ${dx.toFixed(1)}, Δy ${dy.toFixed(1)})`;
    const mx = (ax + bx) / 2, my = (ay + by) / 2;
    ctx.font = "12px system-ui";
    const tw = ctx.measureText(label).width;
    ctx.fillStyle = "rgba(20,21,24,0.85)";
    ctx.fillRect(mx - tw / 2 - 6, my - 22, tw + 12, 18);
    ctx.fillStyle = "#f0a500";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(label, mx, my - 13);
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
  }

  // Marker fuer die zuletzt gelesene Laser-Position: Kopf (Fadenkreuz) und
  // Benutzerursprung (Ring). Beide in mm, via toScreen positioniert.
  function drawLaserMarkers(ctx: CanvasRenderingContext2D) {
    if (laserOrigin) {
      const [x, y] = toScreen(laserOrigin[0], laserOrigin[1]);
      ctx.strokeStyle = "#f0a500";
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.arc(x, y, 7, 0, Math.PI * 2);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(x, y, 2, 0, Math.PI * 2);
      ctx.fillStyle = "#f0a500";
      ctx.fill();
    }
    if (laserHead) {
      const [x, y] = toScreen(laserHead[0], laserHead[1]);
      ctx.strokeStyle = "#3fb27f";
      ctx.lineWidth = 1.5;
      const r = 8;
      ctx.beginPath();
      ctx.moveTo(x - r, y);
      ctx.lineTo(x + r, y);
      ctx.moveTo(x, y - r);
      ctx.lineTo(x, y + r);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  // Vorschau des laufenden Polylinien-Zugs: gesetzte Segmente, Gummiband zur
  // Maus und kleine Marker an den Stuetzpunkten.
  function drawPolyPreview(ctx: CanvasRenderingContext2D) {
    if (tool !== "polyline" && tool !== "spline" || polyPts.length === 0) return;
    ctx.strokeStyle = "rgba(255,255,255,0.7)";
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    polyPts.forEach((p, i) => {
      const [px, py] = toScreen(p[0], p[1]);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    });
    ctx.stroke();
    // Gummiband vom letzten Punkt zur Maus (gestrichelt). Sind wir im Fangradius
    // des Startpunkts, zieht das Band stattdessen zum ersten Punkt (Schliessen).
    if (polyCursor) {
      const [lx, ly] = toScreen(...polyPts[polyPts.length - 1]);
      const [tx, ty] = polyNearStart
        ? toScreen(...polyPts[0])
        : toScreen(...polyCursor);
      ctx.setLineDash([5, 4]);
      ctx.strokeStyle = "rgba(255,255,255,0.4)";
      ctx.beginPath();
      ctx.moveTo(lx, ly);
      ctx.lineTo(tx, ty);
      ctx.stroke();
      ctx.setLineDash([]);
    }
    // Punkt-Marker. Der erste Punkt wird hervorgehoben, sobald der Cursor ihn
    // fangen kann — Signal, dass ein Klick die Kontur schliesst.
    polyPts.forEach((p, i) => {
      const [px, py] = toScreen(p[0], p[1]);
      if (i === 0 && polyNearStart) {
        // Groesserer, andersfarbiger Ring + Fuellung.
        ctx.fillStyle = "#3fb27f";
        ctx.strokeStyle = "#3fb27f";
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(px, py, 6, 0, Math.PI * 2);
        ctx.fill();
        ctx.beginPath();
        ctx.arc(px, py, 9, 0, Math.PI * 2);
        ctx.stroke();
      } else {
        ctx.fillStyle = "#4c82f7";
        ctx.fillRect(px - 3, py - 3, 6, 6);
      }
    });
  }

  // Grid + Bett zeichnet jetzt die WebGL-Ebene (gridBatchGl/bedBatchGl).

  function layerColor(s: Shape): string {
    const l = scene.layers[s.layer_id];
    return l ? rgb(l.color) : "#ff5c62";
  }
  // Füllfarbe der Layer-Farbe mit Alpha (echtes rgba — an rgb() darf kein
  // Hex-Alpha angehaengt werden, das ergaebe einen ungueltigen fillStyle).
  function layerFillColor(s: Shape, alpha: number): string {
    const l = scene.layers[s.layer_id];
    const [r, g, b] = l ? l.color : [255, 92, 98];
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }
  function layerFilled(s: Shape): boolean {
    const l = scene.layers[s.layer_id];
    // Image-Layer nicht flaechig einfaerben — das Bild wird selbst gezeichnet.
    return !!l && (l.mode === "Fill" || l.mode === "Raster");
  }

  // Wendet das laufende Move-/Scale-Delta (nur visuell, waehrend der Geste) auf
  // EINEN Weltpunkt der Form `idx` an. So wandert die eigentliche Geometrie
  // (auch Polylinien-Punkte) live mit — nicht nur die Bounding-Box.
  function liveTransformPoint(px: number, py: number, idx: number): [number, number] {
    if (drag?.kind === "move" && scene.selected.includes(idx)) {
      return [px + drag.dx, py + drag.dy];
    }
    if (drag?.kind === "scale" && scene.selected.includes(idx)) {
      const [sx, sy, sw, sh] = drag.start;
      const [tx, ty, tw, th] = drag.cur;
      const fx = sw > 0 ? tw / sw : 1, fy = sh > 0 ? th / sh : 1;
      return [tx + (px - sx) * fx, ty + (py - sy) * fy];
    }
    return [px, py];
  }

  // Live-Verschiebe-/Skalier-Box einer Form beim Ziehen (nur visuell). Baut auf
  // liveTransformPoint auf, damit Box und Geometrie garantiert deckungsgleich
  // wandern (obere-linke + untere-rechte Ecke transformieren).
  function shapeDrawBox(s: Shape, idx: number): [number, number, number, number] {
    const [x, y, w, h] = shapeBBox(s);
    const [ax, ay] = liveTransformPoint(x, y, idx);
    const [bx, by] = liveTransformPoint(x + w, y + h, idx);
    return [Math.min(ax, bx), Math.min(ay, by), Math.abs(bx - ax), Math.abs(by - ay)];
  }

  // 2D-Ebene je Shape: Bilder (drawImage-Quad) + Flächen gefüllter Layer. Die
  // KONTUR selbst zeichnet die WebGL-Ebene (renderGeo); hier nur Füllung/Bild.
  function drawFill(ctx: CanvasRenderingContext2D, s: Shape, idx: number) {
    const [bx, by, bw, bh] = shapeDrawBox(s, idx);
    // Bild: gecachtes Bitmap in die Box zeichnen (volle Aufloesung, ADR 0004).
    // Move/Resize aendern nur die Box — kein Neurendern.
    if ("Image" in s.geo) {
      ctx.save();
      if (s.rotation) {
        const [scx, scy] = toScreen(bx + bw / 2, by + bh / 2);
        ctx.translate(scx, scy); ctx.rotate((s.rotation * Math.PI) / 180); ctx.translate(-scx, -scy);
      }
      const [sx, sy] = toScreen(bx, by);
      const { asset, params } = s.geo.Image;
      const img = cachedImage(asset, params);
      if (img) ctx.drawImage(img, sx, sy, bw * zoom, bh * zoom);
      else { ctx.fillStyle = layerFillColor(s, 0.15); ctx.fillRect(sx, sy, bw * zoom, bh * zoom); }
      ctx.strokeStyle = layerColor(s); ctx.lineWidth = 1;
      ctx.strokeRect(sx, sy, bw * zoom, bh * zoom);
      ctx.restore();
      return;
    }
    if (!layerFilled(s)) return; // nur gefüllte Layer bekommen eine Fläche
    ctx.save();
    if (s.rotation) {
      const [scx, scy] = toScreen(bx + bw / 2, by + bh / 2);
      ctx.translate(scx, scy); ctx.rotate((s.rotation * Math.PI) / 180); ctx.translate(-scx, -scy);
    }
    const [sx, sy] = toScreen(bx, by);
    ctx.fillStyle = layerFillColor(s, 0.32);
    ctx.beginPath();
    if ("Ellipse" in s.geo) {
      ctx.ellipse(sx + (bw * zoom) / 2, sy + (bh * zoom) / 2, (bw * zoom) / 2, (bh * zoom) / 2, 0, 0, Math.PI * 2);
    } else if ("Polyline" in s.geo) {
      const { pts } = s.geo.Polyline;
      pts.forEach((p, i) => {
        const [wx, wy] = liveTransformPoint(p[0], p[1], idx);
        const [px, py] = toScreen(wx, wy);
        if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
      });
      ctx.closePath();
    } else {
      ctx.rect(sx, sy, bw * zoom, bh * zoom);
    }
    ctx.fill();
    ctx.restore();
  }

  // Ecken einer Vektor-Shape in mm (Rotation eingerechnet) für den Fillet-Pick.
  function shapeCorners(s: Shape): [number, number][] {
    let pts: [number, number][] = [];
    if ("Rect" in s.geo) {
      const { x, y, w, h } = s.geo.Rect;
      pts = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    } else if ("Polyline" in s.geo) {
      pts = s.geo.Polyline.pts.map(([a, b]) => [a, b]);
    } else {
      return [];
    }
    if (s.rotation) {
      const [bx, by, bw, bh] = shapeBBox(s);
      const cx = bx + bw / 2, cy = by + bh / 2;
      const rad = (s.rotation * Math.PI) / 180;
      const co = Math.cos(rad), si = Math.sin(rad);
      pts = pts.map(([x, y]) => [cx + (x - cx) * co - (y - cy) * si, cy + (x - cx) * si + (y - cy) * co]);
    }
    return pts;
  }

  function drawFilletMarkers(ctx: CanvasRenderingContext2D) {
    if (!filletpick) return;
    scene.shapes.forEach((s, i) => {
      for (const [c, [wx, wy]] of shapeCorners(s).entries()) {
        const [px, py] = toScreen(wx, wy);
        const on = filletsel.includes(`${i}:${c}`);
        ctx.beginPath();
        ctx.arc(px, py, on ? 6 : 4.5, 0, Math.PI * 2);
        ctx.fillStyle = on ? "#f0a500" : "rgba(255,255,255,0.85)";
        ctx.fill();
        ctx.strokeStyle = "#1c1e24";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    });
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
      ctx.strokeStyle = "rgba(255,255,255,0.6)";
      ctx.setLineDash([5, 4]);
      ctx.lineWidth = 1.2;
      if (tool === "line") {
        const [ax, ay] = toScreen(drag.sx, drag.sy);
        const [bx, by] = toScreen(drag.cx, drag.cy);
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
        ctx.stroke();
      } else if (tool === "polygon") {
        // Vorschau-Umriss (Naeherung, nur visuell). Zentrum = Startpunkt,
        // Radius = Abstand zur Maus.
        const r = Math.hypot(drag.cx - drag.sx, drag.cy - drag.sy);
        const pts = polygonPreview(activeShape, drag.sx, drag.sy, r, 0);
        if (pts.length >= 3) {
          ctx.beginPath();
          pts.forEach((p, i) => {
            const [px, py] = toScreen(p[0], p[1]);
            if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
          });
          ctx.closePath();
          ctx.stroke();
        }
      } else {
        const x = Math.min(drag.sx, drag.cx), y = Math.min(drag.sy, drag.cy);
        const w = Math.abs(drag.cx - drag.sx), h = Math.abs(drag.cy - drag.sy);
        const [px, py] = toScreen(x, y);
        if (tool === "ellipse") {
          ctx.beginPath();
          ctx.ellipse(px + (w * zoom) / 2, py + (h * zoom) / 2, (w * zoom) / 2, (h * zoom) / 2, 0, 0, Math.PI * 2);
          ctx.stroke();
        } else {
          ctx.strokeRect(px, py, w * zoom, h * zoom);
        }
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

  async function onPointerDown(ev: PointerEvent) {
    canvasEl.setPointerCapture(ev.pointerId);
    const [px, py] = localXY(ev);
    // Mittel-Maus oder Space = Pan.
    if (ev.button === 1) {
      drag = { kind: "pan", px, py, ox: panX, oy: panY };
      return;
    }
    if (ev.button !== 0) return;
    const [mx, my] = toMm(px, py);

    if (tool === "bezier") {
      const CLOSE = 10;
      if (!bez) {
        bez = { nodes: [{ p: [mx, my], hIn: null, hOut: null }], cursor: [mx, my], closed: false };
        bezDrag = 0;
      } else {
        // Nahe Startknoten (>=2 Knoten) → schließen + fertig.
        const [sx, sy] = toScreen(...bez.nodes[0].p);
        if (bez.nodes.length >= 2 && Math.hypot(sx - px, sy - py) < CLOSE) {
          bez.closed = true;
          bezCommit();
          return;
        }
        const nd: BNode = { p: [mx, my], hIn: null, hOut: null };
        bez.nodes = [...bez.nodes, nd];
        bezDrag = bez.nodes.length - 1;
      }
      bezDragStart = [px, py];
      bezDragged = false;
      draw();
      return;
    }
    if (tool === "node") {
      const hit = hitNode(px, py);
      if (hit) {
        if (ev.altKey && hit.part === "anchor") ondeletenode?.(hit.shape, hit.node);
        else drag = { kind: "node", ...hit, began: false };
      } else {
        const segment = await onhitnodesegment?.(mx, my, 8 / zoom);
        if (segment) {
          onsplitnode?.(segment.shape, segment.segment, segment.t);
          return;
        }
        // Kein Knoten getroffen → Shape unter dem Cursor selektieren (zum Editieren).
        onselectat(mx, my, ev.shiftKey || ev.ctrlKey);
      }
      return;
    }
    if (bridgepick) {
      drag = { kind: "bridge", sx: mx, sy: my, cx: mx, cy: my };
      draw();
      return;
    }
    if (filletpick) {
      // Nächstgelegene Ecke innerhalb 10 px togglen.
      let best: [number, number] | null = null;
      let bestD = 10;
      scene.shapes.forEach((s, i) => {
        for (const [c, [wx, wy]] of shapeCorners(s).entries()) {
          const [cx2, cy2] = toScreen(wx, wy);
          const d = Math.hypot(cx2 - px, cy2 - py);
          if (d < bestD) {
            bestD = d;
            best = [i, c];
          }
        }
      });
      if (best) onfilletcorner?.(best[0], best[1]);
      return;
    }
    if (tool === "measure") {
      measureA = [mx, my];
      measureB = [mx, my];
      drag = { kind: "measure" };
      draw();
      return;
    }
    if (tool === "rect" || tool === "ellipse" || tool === "line" || tool === "polygon") {
      // Polygon: Startpunkt = Zentrum, Ziehen bestimmt den Aussenradius.
      drag = { kind: "draw", sx: mx, sy: my, cx: mx, cy: my };
      return;
    }
    if ((tool === "polyline" || tool === "spline")) {
      // Klick auf den ersten Punkt (im Fangradius) schliesst die Kontur.
      if (nearFirstPoint(px, py)) {
        polyCommit(true);
        return;
      }
      // Sonst: jeder Klick setzt einen Stuetzpunkt (Abschluss per Doppelklick/
      // Enter offen, oder Klick auf den Startpunkt geschlossen).
      polyPts = [...polyPts, [mx, my]];
      polyCursor = [mx, my];
      draw();
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
    // Bei schneller Bewegung fasst WebKitGTK mehrere Bewegungen zu EINEM Event
    // zusammen (Coalescing). `ev` trägt dann eine ältere Position als der Cursor
    // real hat → die Form "springt hinterher". getCoalescedEvents liefert den
    // gepufferten Verlauf; die letzte Position davon ist die aktuellste.
    const co = ev.getCoalescedEvents?.();
    const latest = co && co.length ? co[co.length - 1] : ev;
    const [px, py] = localXY(latest);
    const [mx, my] = toMm(px, py);
    // Bézier-Feder: Ziehen setzt symmetrische Tangenten am eben gesetzten Knoten.
    if (tool === "bezier" && bez) {
      if (bezDrag !== null && bezDragStart) {
        if (!bezDragged && Math.hypot(px - bezDragStart[0], py - bezDragStart[1]) > 3) bezDragged = true;
        if (bezDragged) {
          const node = bez.nodes[bezDrag];
          if (node) {
            const updated: BNode = {
              ...node,
              hOut: [mx, my],
              hIn: [2 * node.p[0] - mx, 2 * node.p[1] - my],
            };
            bez.nodes = bez.nodes.map((n, i) => i === bezDrag ? updated : n);
          }
        }
      } else {
        bez.cursor = [mx, my]; // Gummiband
      }
      drawSync();
      return;
    }
    // Polylinien-Gummiband: Cursor verfolgen, auch ohne aktiven Drag.
    if ((tool === "polyline" || tool === "spline") && polyPts.length > 0) {
      polyCursor = [mx, my];
      polyNearStart = nearFirstPoint(px, py);
      drawSync();
    }
    if (!drag) return;
    if (drag.kind === "pan") {
      viewTouched = true;
      panX = drag.ox + (px - drag.px);
      panY = drag.oy + (py - drag.py);
    } else if (drag.kind === "bridge") {
      drag.cx = mx;
      drag.cy = my;
    } else if (drag.kind === "node") {
      ondragnode?.(drag.shape, drag.node, drag.part, mx, my, !drag.began);
      drag.began = true;
    } else if (drag.kind === "draw" || drag.kind === "marquee") {
      drag = { ...drag, cx: mx, cy: my };
    } else if (drag.kind === "move") {
      drag = { ...drag, dx: mx - drag.sx, dy: my - drag.sy };
    } else if (drag.kind === "scale") {
      const dx = mx - (drag.start[0] + hxOffset(drag.handle, drag.start));
      const dy = my - (drag.start[1] + hyOffset(drag.handle, drag.start));
      drag = { ...drag, cur: resizeBox(drag.start, drag.handle, dx, dy) };
    }
    // Während der Geste synchron zeichnen — kein Frame-Versatz ("Cursor klebt").
    drawSync();
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

  async function onPointerUp(ev: PointerEvent) {
    // Bézier-Feder: Tangenten-Drag beenden (kein drag-Objekt im Spiel).
    if (tool === "bezier" && bezDrag !== null) {
      bezDrag = null;
      bezDragStart = null;
      return;
    }
    if (!drag) return;
    const g = drag;
    // WICHTIG: Bei move/scale bleibt `drag` (und damit die Live-Vorschau an der
    // NEUEN Stelle) bestehen, bis der Core die aktualisierte Scene liefert.
    // Sonst zeigt ein Frame die Form kurz an der alten Position ("aufblitzen"),
    // weil `drag=null` sofort greift, die neue Scene aber erst async ankommt.
    if (g.kind === "measure") {
      drag = null; // Messung bleibt sichtbar, bis neu gemessen/Tool gewechselt
      return;
    }
    if (g.kind === "node") {
      drag = null;
      return;
    }
    if (g.kind === "bridge") {
      drag = null;
      onbridgestroke?.(g.sx, g.sy, g.cx, g.cy);
      return;
    }
    if (g.kind === "draw") {
      drag = null;
      if (tool === "line") {
        // Echte Endpunkte A→B, Mindestlänge 1 mm.
        const len = Math.hypot(g.cx - g.sx, g.cy - g.sy);
        if (len > 1) ondrawline(g.sx, g.sy, g.cx, g.cy);
      } else if (tool === "polygon") {
        // Radius aus der Ziehstrecke; Mindestradius 1 mm. Der Core erzeugt die
        // echte Form (add_polygon) — die Vorschau war nur eine Naeherung.
        const r = Math.hypot(g.cx - g.sx, g.cy - g.sy);
        if (r > 1) ondrawpolygon(activeShape, g.sx, g.sy, r, 0);
      } else {
        const x = Math.min(g.sx, g.cx), y = Math.min(g.sy, g.cy);
        const w = Math.abs(g.cx - g.sx), h = Math.abs(g.cy - g.sy);
        if (w > 1 && h > 1) {
          if (tool === "ellipse") ondrawellipse(x + w / 2, y + h / 2, w / 2, h / 2);
          else ondrawrect(x, y, w, h);
        }
      }
    } else if (g.kind === "marquee") {
      drag = null;
      const w = Math.abs(g.cx - g.sx), h = Math.abs(g.cy - g.sy);
      if (w > 0.5 || h > 0.5) onselectrect(g.sx, g.sy, g.cx, g.cy);
    } else if (g.kind === "move") {
      // Erst den Core anwenden lassen, DANN die Vorschau loesen — nahtlos.
      if (g.dx !== 0 || g.dy !== 0) await onmove(g.dx, g.dy);
      drag = null;
    } else if (g.kind === "scale") {
      await onscale(g.start, g.cur);
      drag = null;
    } else {
      drag = null;
    }
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

  // ---- Polylinien-Modus: Abschluss / Abbruch --------------------------------
  // Schliesst den aktuellen Zug ab: bei >= 2 Punkten als offene Polylinie an den
  // Core, sonst verwerfen. `closed` schliesst die Kontur.
  function polyCommit(closed = false) {
    if (polyPts.length >= 2) {
      ondrawpolyline([...polyPts], closed);
    }
    polyPts = [];
    polyCursor = null;
    polyNearStart = false;
  }
  function polyCancel() {
    polyPts = [];
    polyCursor = null;
    polyNearStart = false;
    draw();
  }

  // Doppelklick schliesst die Polylinie ab. Der Doppelklick hat ueber
  // pointerdown schon einen (nahezu deckungsgleichen) Extrapunkt gesetzt — den
  // entfernen wir, wenn er auf dem vorherigen liegt.
  function onDblClick(ev: MouseEvent) {
    // Im Polyline-Modus: Kontur abschliessen (wie bisher).
    if (tool === "bezier" && bez && bez.nodes.length >= 2) {
      bezCommit();
      return;
    }
    if ((tool === "polyline" || tool === "spline") && polyPts.length >= 2) {
      const n = polyPts.length;
      const [ax, ay] = polyPts[n - 1];
      const [bx, by] = polyPts[n - 2];
      if (Math.hypot(ax - bx, ay - by) < 0.5) polyPts = polyPts.slice(0, n - 1);
      polyCommit(false);
      return;
    }
    const [px, py] = localXY(ev);
    const [mx, my] = toMm(px, py);
    // Node-Modus: Doppelklick auf einen Knoten löscht ihn; auf ein Segment
    // (Kurve) fügt einen Knoten ein (Segment davor teilen).
    if (tool === "bezier") {
      const CLOSE = 10;
      if (!bez) {
        bez = { nodes: [{ p: [mx, my], hIn: null, hOut: null }], cursor: [mx, my], closed: false };
        bezDrag = 0;
      } else {
        // Nahe Startknoten (>=2 Knoten) → schließen + fertig.
        const [sx, sy] = toScreen(...bez.nodes[0].p);
        if (bez.nodes.length >= 2 && Math.hypot(sx - px, sy - py) < CLOSE) {
          bez.closed = true;
          bezCommit();
          return;
        }
        const nd: BNode = { p: [mx, my], hIn: null, hOut: null };
        bez.nodes = [...bez.nodes, nd];
        bezDrag = bez.nodes.length - 1;
      }
      bezDragStart = [px, py];
      bezDragged = false;
      draw();
      return;
    }
    if (tool === "node") {
      const hit = hitNode(px, py);
      if (hit) {
        if (hit.part === "anchor") ontogglenode?.(hit.shape, hit.node);
        return;
      }
      return;
    }
    // Sonst: Doppelklick auf ein Bild-Shape oeffnet den Bild-Editor,
    // auf einen Text-Block den Text-Editor.
    // Oberstes getroffenes Shape (spaetere liegen oben).
    for (let i = scene.shapes.length - 1; i >= 0; i--) {
      const s = scene.shapes[i];
      const [bx, by, bw, bh] = shapeBBox(s);
      const hit = mx >= bx && mx <= bx + bw && my >= by && my <= by + bh;
      if (!hit) continue;
      if ("Image" in s.geo) {
        oneditimage?.(i);
        return;
      }
      // Text-Block: Meta liegt am ersten Gruppenmitglied.
      if (s.text_meta) {
        onedittext?.(i);
        return;
      }
      if (s.group_id != null) {
        const holder = scene.shapes.findIndex(
          (o) => o.group_id === s.group_id && o.text_meta,
        );
        if (holder >= 0) {
          onedittext?.(holder);
          return;
        }
      }
    }
  }

  function onKeydown(ev: KeyboardEvent) {
    // Bézier-Feder: Enter schließt offen ab, Esc bricht ab.
    if (bez) {
      if (ev.key === "Enter") { ev.preventDefault(); bezCommit(); }
      else if (ev.key === "Escape") { ev.preventDefault(); bez = null; bezDrag = null; draw(); }
      else if (ev.key === "Backspace") {
        ev.preventDefault();
        if (bez.nodes.length > 1) bez.nodes = bez.nodes.slice(0, -1);
        else { bez = null; bezDrag = null; }
        draw();
      }
      return;
    }
    if (polyPts.length === 0) return;
    if (ev.key === "Enter") {
      ev.preventDefault();
      polyCommit(false);
    } else if (ev.key === "Escape") {
      ev.preventDefault();
      polyCancel();
    }
  }

  function resize() {
    if (!wrapEl || !canvasEl) return;
    const nw = wrapEl.clientWidth, nh = wrapEl.clientHeight;
    // WICHTIG: `canvas.width/height` NUR setzen, wenn sich die Groesse wirklich
    // aendert. Das Zuweisen loescht den Canvas komplett (Schwarz-Blitz) — vorher
    // passierte das bei jeder Aktion, weil resize() unkonditioniert lief.
    if (canvasEl.width !== nw || canvasEl.height !== nh) {
      canvasEl.width = nw;
      canvasEl.height = nh;
    }
    // WebGL-Ebene identisch dimensionieren (deckungsgleich mit dem 2D-Layer).
    if (glCanvasEl && (glCanvasEl.width !== nw || glCanvasEl.height !== nh)) {
      glCanvasEl.width = nw;
      glCanvasEl.height = nh;
    }
    // Solange der Nutzer die Ansicht nicht selbst bewegt hat, Bett einpassen.
    if (!viewTouched) fitBed();
    draw();
  }

  // Ein einziger Redraw-Effect fuer alle zeichenrelevanten Zustaende. draw()
  // ist rAF-gedrosselt, mehrere Trigger pro Frame ergeben also einen Redraw.
  $effect(() => {
    scene; zoom; panX; panY; drag;
    polyPts; polyCursor; polyNearStart;
    laserHead; laserOrigin;
    draw();
  });
  // Aendern sich die freien Raender (Reiterwechsel, Panel verschoben) und der
  // Nutzer hat die Ansicht noch nicht selbst bewegt, das Bett neu einpassen.
  $effect(() => {
    insets;
    if (!viewTouched) { fitBed(); draw(); }
  });
  // Wenn der Designer wieder sichtbar wird, die Kamera bewusst neu auf das Bett
  // setzen. Das behebt den Fall Projekt/Preview -> Design mit alter Scrolllage.
  $effect(() => {
    fitTrigger;
    if (active) scheduleFitBed(true);
  });
  // Ändert sich die Scene-Geometrie, den WebGL-Konturen-Batch neu hochladen.
  // NUR wenn keine Live-Drag-Geste läuft — während move/scale baut renderGeo den
  // Batch pro Frame selbst (mit Transform), ein zusätzliches rebuild wäre doppelt.
  $effect(() => {
    scene.shapes; scene.layers;
    if (!drag || (drag.kind !== "move" && drag.kind !== "scale")) rebuildGeo();
  });
  // rAF beim Unmount abbrechen (kein Redraw auf totem Canvas) + GPU-Buffer frei.
  $effect(() => () => {
    if (rafId) cancelAnimationFrame(rafId);
    clearScheduledFit();
    if (gl && shapeBatch) gl.free(shapeBatch);
  });
  // Werkzeugwechsel bricht einen laufenden Polylinien-Zug ab.
  $effect(() => {
    if (tool !== "polyline" && tool !== "spline" && polyPts.length > 0) polyCancel();
    if (tool !== "bezier" && bez) { bez = null; bezDrag = null; }
  });
  onMount(() => {
    resize();
    // Beim (Neu-)Mount die Ansicht sicher einpassen — auch wenn beim ersten
    // resize() das Layout (Canvas-Größe/insets) noch nicht stand. Ohne das
    // musste man nach jedem Tab-Wechsel Preview→Design erst zurückscrollen.
    scheduleFitBed(true);
    const ro = new ResizeObserver(resize);
    if (wrapEl) ro.observe(wrapEl);
    return () => {
      ro.disconnect();
      clearScheduledFit();
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="wrap" bind:this={wrapEl}>
  <!-- Untere Ebene: WebGL-Geometrie (Konturen, Grid, Bett). -->
  <canvas bind:this={glCanvasEl} class="gl"></canvas>
  <!-- Obere Ebene: 2D-Overlays + Bilder; trägt alle Pointer-Handler. -->
  <canvas
    bind:this={canvasEl}
    class="ov"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    ondblclick={onDblClick}
    onwheel={onWheel}
  ></canvas>
</div>

<style>
  .wrap { position: absolute; inset: 0; }
  /* Beide Canvas-Ebenen deckungsgleich übereinander: WebGL (gl) unten,
     2D-Overlay (ov) darüber. Nur der obere Layer nimmt Pointer-Events an —
     der untere liegt unter ihm und braucht sie nicht. */
  canvas {
    display: block;
    position: absolute;
    inset: 0;
    touch-action: none;
    transform: translateZ(0);
    will-change: transform;
  }
  .gl { z-index: 0; }
  .ov { z-index: 1; }
</style>
