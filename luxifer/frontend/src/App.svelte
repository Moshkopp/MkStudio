<script lang="ts">
  import { untrack } from "svelte";
  import Canvas from "./lib/Canvas.svelte";
  import PreviewCanvas from "./lib/PreviewCanvas.svelte";
  import LayerDialog from "./lib/LayerDialog.svelte";
  import LaserPanel from "./lib/LaserPanel.svelte";
  import LaserSettings from "./lib/LaserSettings.svelte";
  import SettingsModal from "./lib/SettingsModal.svelte";
  import ToolsPanel from "./lib/ToolsPanel.svelte";
  import LayersPanel from "./lib/LayersPanel.svelte";
  import PalettePanel from "./lib/PalettePanel.svelte";
  import ShapesPanel from "./lib/ShapesPanel.svelte";
  import ArrangePanel from "./lib/ArrangePanel.svelte";
  import TransformPanel from "./lib/TransformPanel.svelte";
  import ProjectBrowser from "./lib/ProjectBrowser.svelte";
  import ImageEditor from "./lib/ImageEditor.svelte";
  import TextDialog from "./lib/TextDialog.svelte";
  import GeoToolDialog from "./lib/GeoToolDialog.svelte";
  import Icon from "./lib/Icon.svelte";
  import logoUrl from "./assets/logo.png";
  import * as core from "./lib/core";
  import { renderThumbnail } from "./lib/thumbnail";
  import type {
    Scene,
    LayerParams,
    UiSettings,
  } from "./lib/core";
  import { applyTheme } from "./lib/theme";

  type Tab = "Projekt" | "Design" | "Laser" | "Monitor" | "Preview";
  type Tool = "select" | "rect" | "ellipse" | "line" | "polyline" | "polygon" | "spline" | "measure" | "bezier" | "node";

  let scene = $state<Scene | null>(null);
  let tool = $state<Tool>("select");
  let swatches = $state<[number, number, number][]>([]);
  // Formen-Katalog (datengetrieben aus dem Core) + aktuell gewaehlte Form.
  let shapes = $state<core.ShapeInfo[]>([]);
  let activeShape = $state("hex");
  let error = $state<string | null>(null);
  let editLayer = $state<number | null>(null);
  // Bild-Editor (ADR 0004): Index des Bild-Shapes, das gerade bearbeitet wird.
  let editImage = $state<number | null>(null);
  // Versteckter Datei-Input fuer den Bild-Import (per Button ausgeloest).
  let fileInput = $state<HTMLInputElement | null>(null);
  let status = $state<string | null>(null);
  let stopErrorListener: (() => void) | null = null;

  // Alle Tauri-/Core-Fehler laufen durch denselben Kanal, auch wenn der Aufruf
  // aus einer modalen Unterkomponente stammt.
  stopErrorListener = core.onCommandError((e) => {
    error = e.message;
    console.error(`[${e.code}] ${e.command ?? "editor"}: ${e.message}`, e.details ?? "");
  });
  $effect(() => () => stopErrorListener?.());

  // --- Laser (ADR 0007) -----------------------------------------------------
  let laserReg = $state<core.LaserRegistry | null>(null);
  let laserActions = $state<string[]>([]);
  let showLaserSettings = $state(false);
  // Zentrales Einstellungs-Modal (Zahnrad oben rechts).
  let showSettings = $state(false);
  // Verbindungsstatus (LED) + gelesene Positionen (Canvas-Marker).
  let laserConnected = $state(false);
  let laserHead = $state<[number, number] | null>(null);
  let laserOrigin = $state<[number, number] | null>(null);
  let laserJobStart = $state<[number, number] | null>(null);
  let laserJobParams = $state<core.JobParamsDto>({ start_mode: "absolut", anchor: 4, selection_only: false });

  // --- Projektverwaltung (ADR 0003) -----------------------------------------
  // saveMode: Projekt-Reiter zeigt das Speichern-Formular (Strg+S bei namenlos).
  let saveMode = $state(false);
  // Start-Toast „zuletzt gearbeitet an …". Verschwindet, sobald der Nutzer
  // irgendetwas tut (Tool/Tab/Aktion) — nicht nur bei Öffnen/Verwerfen.
  let startToast = $state<string | null>(null);
  // Wird true, sobald der Toast steht; erst dann räumt ihn die erste Interaktion
  // weg (verhindert, dass der Effect ihn beim Mount sofort wieder schließt).
  let startToastArmed = false;
  // Unsaved-Guard: geplante Aktion (open/new), die nach Bestaetigung laeuft.
  let pendingAction = $state<null | { kind: "new" } | { kind: "open"; name: string }>(null);

  // --- Splash / App-Version -------------------------------------------------
  // Der Splash ist ein eigenes Fenster (klassische Reihenfolge: Splash zuerst,
  // Hauptfenster startet unsichtbar). Wenn die GUI geladen ist, meldet sich das
  // Frontend per frontendReady() → Backend zeigt `main` und schließt den Splash.
  // Mindest-Standzeit, damit der Splash auch bei blitzschnellem Start sichtbar
  // bleibt. Der Zeitpunkt zählt ab App-Start (Modul-Init).
  let appVer = $state<core.AppVersion>({ version: "", commit: "" });
  const splashStart = Date.now();

  // Zeigt das Hauptfenster / schließt den Splash — frühestens nach der in den
  // Settings hinterlegten Mindest-Standzeit. Idempotent (frontendReady ist
  // backend-seitig idempotent).
  function revealMain(minMs: number) {
    const rest = minMs - (Date.now() - splashStart);
    setTimeout(() => core.frontendReady().catch(() => {}), Math.max(0, rest));
  }

  // --- GUI-Settings (Theme/Arbeitsplatz, ADR 0002) --------------------------
  let settings = $state<UiSettings | null>(null);
  let activeTab = $state<Tab>("Design");
  let designFitTrigger = $state(0);
  let wasDesignVisible = false;

  // Start-Toast bei der ersten echten Interaktion schließen: jede Tool- oder
  // Tab-Änderung reicht (nach dem Scharfschalten in load()). Aktionen ohne
  // Tool-/Tab-Wechsel schließen ihn zusätzlich über dismissStartToast().
  // WICHTIG: nur tool/activeTab tracken. Würde der Effect `startToast` lesen,
  // triggerte dessen Setzen in load() ihn erneut und schlösse den Toast sofort.
  $effect(() => {
    tool; activeTab; // einzige getrackte Abhängigkeiten
    untrack(() => {
      if (startToastArmed && startToast) startToast = null;
    });
  });
  // Explizit aus Aktions-Handlern (Anordnen, Layer, Speichern …), die weder Tool
  // noch Tab wechseln, aber trotzdem „irgendeine Funktion" sind.
  function dismissStartToast() {
    if (startToast) startToast = null;
  }
  // Zweireihige Topbar (Design, mit Anordnen-Zeile) vs. einreihige (Laser/Monitor).
  const TOPBAR_H = 92;
  const TOPBAR_H1 = 48;
  const LEFT_DOCK_W = 108;
  // Rechter Dock: 316px Breite + 12px Luft (siehe --right-dock-w im CSS).
  const RIGHT_DOCK_W = 328;

  async function load() {
    try {
      appVer = await core.appVersion();
      scene = await core.getScene();
      swatches = await core.swatchColors();
      shapes = await core.shapeCatalog();
      settings = await core.getUiSettings();
      applyTheme(settings.theme);
      // Hauptfenster zeigen: bei deaktiviertem Splash sofort, sonst nach der
      // Mindest-Standzeit (das Backend zeigt `main` und schließt den Splash).
      if (!settings.show_splash) core.frontendReady().catch(() => {});
      else revealMain(settings.splash_ms ?? 1500);
      // Start-Toast: zuletzt geoeffnetes Projekt anbieten (ADR 0003 §3).
      if (settings.last_project) {
        startToast = settings.last_project;
        // Nächster Tick: scharf schalten, damit die nächste echte Tool-/Tab-
        // Änderung (nicht der initiale Effect-Lauf) den Toast schließt.
        queueMicrotask(() => (startToastArmed = true));
      }
      await loadLasers();
      // Verbindungs-LED periodisch aktualisieren (nur wenn ein Laser aktiv ist).
      refreshConnection();
      setInterval(refreshConnection, 3000);
    } catch (e) {
      error = core.errorMessage(e);
      core.frontendReady().catch(() => {}); // Fenster nie verstecken lassen.
    }
  }
  load();

  // FitView genau dann neu anstossen, wenn der Designer sichtbar wird:
  // beim ersten Laden und nach Projekt/Laser/Monitor/Preview -> Design.
  $effect(() => {
    const visible = activeTab === "Design" && scene !== null;
    if (visible && !wasDesignVisible) designFitTrigger += 1;
    wasDesignVisible = visible;
  });

  // Freie Raender (px) fuer die Bett-Einpassung im Canvas. Mit statischem Layout
  // gibt es keine Heuristik mehr aus verschiebbaren Panel-Rechtecken.
  const insets = $derived.by(() => {
    if (activeTab === "Design") {
      return { top: TOPBAR_H, right: RIGHT_DOCK_W, bottom: 88, left: tool === "polygon" ? 304 : LEFT_DOCK_W };
    }
    if (activeTab === "Laser") {
      return { top: TOPBAR_H1, right: RIGHT_DOCK_W, bottom: 0, left: RIGHT_DOCK_W };
    }
    if (activeTab === "Monitor") {
      return { top: TOPBAR_H1, right: RIGHT_DOCK_W, bottom: 0, left: 0 };
    }
    return { top: TOPBAR_H1, right: 0, bottom: 0, left: 0 };
  });

  async function persist(next: UiSettings) {
    settings = next;
    applyTheme(next.theme);
    try {
      settings = await core.saveUiSettings(next);
      applyTheme(settings.theme);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  // --- Canvas-Callbacks -----------------------------------------------------
  async function ondrawrect(x: number, y: number, w: number, h: number) {
    scene = await core.addRect(x, y, w, h);
  }
  async function ondrawellipse(cx: number, cy: number, rx: number, ry: number) {
    scene = await core.addEllipse(cx, cy, rx, ry);
  }
  async function ondrawline(x1: number, y1: number, x2: number, y2: number) {
    scene = await core.addLine(x1, y1, x2, y2);
  }
  async function ondrawpolyline(pts: [number, number][], closed: boolean) {
    // Spline/Bézier nutzen denselben Zeichenfluss; Glättung/Knoten im Core.
    scene =
      tool === "bezier"
        ? await core.addBezier(pts, closed)
        : tool === "spline"
          ? await core.addSpline(pts, closed)
          : await core.addPolyline(pts, closed);
  }
  async function ondrawpolygon(shape: string, cx: number, cy: number, r: number, rot: number) {
    scene = await core.addPolygon(shape, cx, cy, r, rot);
  }
  // Form in der Galerie gewaehlt: Form merken und aufs Polygon-Werkzeug wechseln.
  function pickShape(id: string) {
    activeShape = id;
    tool = "polygon";
  }
  async function onselectat(x: number, y: number, additive: boolean) {
    const selection = await core.selectAt(x, y, 2, additive);
    if (!scene) return;
    // Auswahl-only-Antwort darf die Scene-Identität nicht ersetzen: Sonst
    // bewertet Svelte Geometrie-Effects neu und lädt 125k Punkte erneut hoch.
    scene.selected = selection.selected;
    scene.selection_bbox = selection.selection_bbox;
  }
  async function onselectrect(x1: number, y1: number, x2: number, y2: number) {
    const selection = await core.selectRect(x1, y1, x2, y2);
    if (!scene) return;
    scene.selected = selection.selected;
    scene.selection_bbox = selection.selection_bbox;
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
  async function onrotate(degrees: number) {
    scene = await core.rotateSelected(degrees);
  }

  // Datei importieren: Bilder (ADR 0004) gehen in den Asset-Store, Vektor-
  // Dateien (SVG/DXF) werden zu Polylinien auf dem aktiven Layer. Die Endung
  // entscheidet — EIN Import-Knopf für alles.
  async function onImportFile(ev: Event) {
    const input = ev.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = ""; // erlaubt erneutes Waehlen derselben Datei
    if (!file) return;
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const ext = file.name.split(".").pop()?.toLowerCase();
      scene =
        ext === "svg" || ext === "dxf"
          ? await core.importVectorFile(bytes, file.name)
          : await core.importImageFile(bytes, file.name);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  // Das aktuell im Editor bearbeitete Bild-Shape (oder null). Liefert Asset-ID
  // und Parameter für den Dialog.
  const editImageShape = $derived.by(() => {
    if (editImage === null || !scene) return null;
    const s = scene.shapes[editImage];
    if (s && "Image" in s.geo) return s.geo.Image;
    return null;
  });

  // Live-Übernahme der Bild-Parameter aus dem Editor in den Core.
  async function applyImageParams(pp: core.ImageParams) {
    if (editImage === null) return;
    scene = await core.setImageParams(editImage, pp);
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
  // Geometrie-Werkzeuge (Boolean/Offset/Fillet) — wirken auf die Auswahl.
  async function doBoolean(op: core.BoolOpKind) {
    scene = await core.booleanOp(op);
  }
  async function doOffset(dist: number) {
    scene = await core.offsetOp(dist);
  }
  async function doFillet(radius: number) {
    scene = await core.filletOp(radius);
  }
  async function doNest(gap: number) {
    scene = await core.nestOp(gap);
  }
  // Kanonische Welt-BBox kommt direkt aus dem Core.
  const selBBox = $derived(scene?.selection_bbox ?? null);
  async function doTransform(start: [number, number, number, number], target: [number, number, number, number]) {
    scene = await core.scaleSelected(start, target);
  }
  // Bild vektorisieren (aus dem Bild-Editor). Schließt den Dialog bei Erfolg.
  async function doTraceImage(threshold: number, invert: boolean) {
    if (editImage === null) return;
    try {
      scene = await core.traceImage(editImage, threshold, invert);
      editImage = null;
    } catch (e) {
      console.error("Trace fehlgeschlagen:", e);
    }
  }
  // Sofort-Befehle aus der Werkzeugleiste. Spiegeln wirkt auf die Auswahl;
  // "text" öffnet den Text-Dialog (Text→Pfad).
  async function doToolAction(
    a:
      | "mirror_h"
      | "mirror_v"
      | "text"
      | "boolean"
      | "fillet"
      | "offset"
      | "pattern-fill"
      | "coaster_rect"
      | "coaster_circle"
      | "bridge",
  ) {
    dismissStartToast();
    if (a === "text") {
      textOpen = true;
      return;
    }
    if (a === "coaster_rect" || a === "coaster_circle") {
      scene = await core.insertCoasters(a === "coaster_circle");
      return;
    }
    if (a === "bridge") {
      bridgePick = !bridgePick;
      return;
    }
    // Geometrie-Werkzeuge (Referenz-UX): Button öffnet die Optionen,
    // angewendet wird auf die Auswahl.
    if (a === "boolean" || a === "fillet" || a === "offset") {
      geoTool = a;
      return;
    }
    if (a === "pattern-fill") {
      geoTool = "pattern";
      return;
    }
    scene = await core.mirror(a === "mirror_h" ? "h" : "v");
  }
  // Offener Geometrie-Werkzeug-Dialog (oder null).
  let geoTool = $state<null | "boolean" | "fillet" | "offset" | "pattern">(null);
  // Ecken-Pick-Modus fürs Fillet (Referenz-UX: Ecken einzeln anklicken).
  let filletPick = $state(false);
  // Haltesteg-Modus: Klick auf Konturen schneidet Lücken.
  let bridgePick = $state(false);
  let bridgeWidth = $state(2.0);
  async function doBridgeStroke(x0: number, y0: number, x1: number, y1: number) {
    scene = await core.bridgeOp(x0, y0, x1, y1, bridgeWidth);
  }
  let filletRadius = $state(2.0);
  let filletCorners = $state<string[]>([]);
  function toggleFilletCorner(shape: number, corner: number) {
    const key = `${shape}:${corner}`;
    filletCorners = filletCorners.includes(key)
      ? filletCorners.filter((k) => k !== key)
      : [...filletCorners, key];
  }
  async function applyFilletCorners() {
    // Nach Shape gruppieren und je Shape anwenden (absteigend — Indizes stabil,
    // da Fillet die Shape ersetzt, nicht entfernt).
    const byShape = new Map<number, number[]>();
    for (const k of filletCorners) {
      const [si, ci] = k.split(":").map(Number);
      byShape.set(si, [...(byShape.get(si) ?? []), ci]);
    }
    for (const [si, corners] of byShape) {
      scene = await core.filletCornersOp(si, corners, filletRadius);
    }
    filletCorners = [];
    filletPick = false;
  }
  async function doPatternFill(
    p: core.PatternKind,
    gapX: number,
    gapY: number,
    angle: number,
    size: number,
  ) {
    try {
      scene = await core.patternFillOp(p, gapX, gapY, angle, size);
      geoTool = null;
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  let textOpen = $state(false);
  // Text-Edit (Doppelklick): Index des Shapes mit text_meta, oder null (= neu).
  let textEdit = $state<number | null>(null);
  const textEditMeta = $derived.by(() => {
    if (textEdit === null || !scene) return null;
    return scene.shapes[textEdit]?.text_meta ?? null;
  });
  async function doInsertText(text: string, fontPath: string, sizeMm: number) {
    try {
      scene =
        textEdit !== null
          ? await core.updateText(textEdit, text, fontPath, sizeMm)
          : await core.addText(text, fontPath, sizeMm);
      textOpen = false;
      textEdit = null;
    } catch (e) {
      console.error("Text einfügen fehlgeschlagen:", e);
    }
  }
  function openTextEdit(i: number) {
    textEdit = i;
    textOpen = true;
  }
  async function saveLayer(p: LayerParams) {
    if (editLayer !== null) {
      scene = await core.setLayerParams(editLayer, p);
      editLayer = null;
    }
  }
  async function toggleLayer(i: number, field: core.LayerToggle) {
    scene = await core.toggleLayer(i, field);
  }
  // Layer in der Brenn-Reihenfolge verschieben (ADR 0005 §0, Drag & Drop).
  async function moveLayer(from: number, to: number) {
    scene = await core.moveLayer(from, to);
  }
  // --- Laser-Profile & Aktionen (ADR 0007) ----------------------------------
  async function loadLasers() {
    try {
      laserReg = await core.laserList();
      laserActions = await core.laserActions();
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function selectLaser(id: string) {
    try {
      laserReg = await core.laserSetActive(id);
      laserActions = await core.laserActions();
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function runLaserAction(action: string, params: core.JobParamsDto) {
    try {
      status = await core.laserRunAction(action, params);
      setTimeout(() => (status = null), 4000);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function updateLaserJobStart(params: core.JobParamsDto) {
    laserJobParams = params;
    await refreshLaserJobStart(params);
  }
  async function refreshLaserJobStart(params: core.JobParamsDto) {
    try {
      laserJobStart = await core.laserJobStart(params);
    } catch {
      laserJobStart = null;
    }
  }
  $effect(() => {
    // Eine geänderte Auswahl/Geometrie verändert die effektive Job-BBox.
    scene;
    void refreshLaserJobStart(laserJobParams);
  });
  // Job als Datei herunterladen (.rd bzw. .gcode) — kein natives Plugin nötig.
  async function exportLaser(params: core.JobParamsDto) {
    try {
      const dto = await core.laserExport(params);
      const blob = new Blob([new Uint8Array(dto.bytes)], {
        type: "application/octet-stream",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = dto.filename;
      a.click();
      URL.revokeObjectURL(url);
      status = `Exportiert: ${dto.filename} (${dto.bytes.length} Byte)`;
      setTimeout(() => (status = null), 4000);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function jogLaser(dx: number, dy: number, speed: number) {
    try {
      await core.laserJog(dx, dy, speed);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function homeLaser(speed: number) {
    try {
      await core.laserHome(speed);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  // Kopf- und Ursprungsposition lesen und als Marker im Canvas zeigen.
  async function readLaserPosition() {
    try {
      const p = await core.laserPosition();
      laserHead = p.head;
      laserOrigin = p.origin;
      laserConnected = true;
      status = `Kopf ${p.head[0].toFixed(1)}/${p.head[1].toFixed(1)} mm`;
      setTimeout(() => (status = null), 4000);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  // Verbindungs-LED: periodisch pingen, solange ein Laser aktiv ist.
  async function refreshConnection() {
    if (!laserReg?.active_id) {
      laserConnected = false;
      return;
    }
    try {
      laserConnected = await core.laserPing();
    } catch {
      laserConnected = false;
    }
  }
  async function saveLaser(profile: core.LaserProfile) {
    try {
      laserReg = await core.laserSave(profile);
      laserActions = await core.laserActions();
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function deleteLaser(id: string) {
    try {
      laserReg = await core.laserDelete(id);
      laserActions = await core.laserActions();
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  const selCount = $derived(scene?.selected.length ?? 0);
  // Nie-null-Sicht auf die Ebenen fuers Snippet (Snippets erben kein Narrowing).
  const sceneLayers = $derived(scene?.layers ?? []);
  async function doUndo() {
    scene = await core.undo();
  }
  async function doRedo() {
    scene = await core.redo();
  }
  async function doDelete() {
    scene = await core.deleteSelected();
  }

  // --- Projektverwaltung-Handler --------------------------------------------

  // Strg+S: namenlos → Projekt-Reiter mit Speichern-Formular; benannt → still
  // speichern + Toast (bleibt im Designer).
  async function saveShortcut() {
    if (scene?.project) {
      // Bereits benannt: still speichern.
      try {
        const thumb = await renderThumbnail(scene);
        scene = await core.saveProject(
          scene.project.name,
          scene.project.description,
          scene.project.tags,
          thumb,
        );
        flash("Gespeichert ✓ · Shift+Strg+S legt eine Version an");
      } catch (e) {
        error = core.errorMessage(e);
      }
    } else {
      // Namenlos: Projekt-Reiter zum Benennen öffnen.
      saveMode = true;
      activeTab = "Projekt";
    }
  }

  // Aus dem Projekt-Reiter (Formular oder Detail-„Speichern"): mit Metadaten sichern.
  async function saveWithMeta(name: string, description: string, tags: string[]) {
    if (!scene) return;
    const thumb = await renderThumbnail(scene);
    scene = await core.saveProject(name, description, tags, thumb);
    flash("Gespeichert ✓");
  }

  // Shift+Strg+S: neue Version festhalten. Nur bei benanntem Projekt.
  async function saveVersionShortcut() {
    if (!scene) return;
    if (!scene.project) {
      // Noch namenlos → erst benennen.
      saveMode = true;
      activeTab = "Projekt";
      return;
    }
    try {
      const thumb = await renderThumbnail(scene);
      scene = await core.saveVersion("", thumb);
      flash("Version festgehalten ✓");
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  // Öffnen/Neu mit Unsaved-Guard.
  async function requestOpen(name: string) {
    if (scene?.dirty) pendingAction = { kind: "open", name };
    else await doOpen(name);
  }
  async function requestNew() {
    if (scene?.dirty) pendingAction = { kind: "new" };
    else await doNew();
  }
  async function doOpen(name: string) {
    try {
      scene = await core.openProject(name);
      saveMode = false;
      activeTab = "Design";
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function doNew() {
    try {
      scene = await core.newProject();
      saveMode = false;
      activeTab = "Design";
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  async function openVersion(name: string, versionId: string) {
    try {
      scene = await core.openVersion(name, versionId);
      activeTab = "Design";
    } catch (e) {
      error = core.errorMessage(e);
    }
  }
  // Version löschen: der Core liefert die neue Scene zurück (bei gelöschter
  // aktueller Version wird die vorherige geladen). Kein Tab-Wechsel — der Nutzer
  // bleibt im Browser und sieht die aktualisierte Versionsliste.
  async function deleteVersion(name: string, versionId: string) {
    scene = await core.deleteVersion(name, versionId);
  }

  // Unsaved-Guard aufgelöst.
  async function guardDiscard() {
    const a = pendingAction;
    pendingAction = null;
    if (a?.kind === "open") await doOpen(a.name);
    else if (a?.kind === "new") await doNew();
  }
  async function guardSave() {
    // Benannt → still speichern und dann die Aktion ausführen; namenlos →
    // „Speichern unter…“ (Formular), Aktion verwerfen (Nutzer entscheidet neu).
    if (scene?.project) {
      await saveShortcut();
      await guardDiscard();
    } else {
      pendingAction = null;
      saveMode = true;
      activeTab = "Projekt";
    }
  }
  function guardCancel() {
    pendingAction = null;
  }

  // Start-Toast-Aktionen.
  async function toastOpen() {
    const name = startToast;
    startToast = null;
    if (name) await requestOpen(name);
  }

  // Kurze Statusmeldung, die nach ein paar Sekunden verschwindet.
  function flash(msg: string) {
    status = msg;
    setTimeout(() => (status = null), 3000);
  }

  // Globale Tastatur-Kuerzel. Nicht ausloesen, waehrend ein Eingabefeld den
  // Fokus hat (IP, Layer-Name, Zahlenfelder), sonst kann man dort nichts loeschen.
  function isTyping(t: EventTarget | null): boolean {
    const el = t as HTMLElement | null;
    if (!el) return false;
    const tag = el.tagName;
    return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
  }
  async function onKeydown(e: KeyboardEvent) {
    if (isTyping(e.target)) return;
    // Entf / Rueckschritt loescht die Auswahl.
    if (e.key === "Delete" || e.key === "Backspace") {
      if (activeTab !== "Design") return;
      if (selCount > 0) {
        e.preventDefault();
        await doDelete();
      }
      return;
    }
    // Strg-Kombinationen: Undo/Redo + Projekt-Shortcuts (ADR 0003).
    if ((e.ctrlKey || e.metaKey) && !e.altKey) {
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) {
        e.preventDefault();
        if (activeTab === "Design") await doUndo();
      } else if (k === "y" || (k === "z" && e.shiftKey)) {
        e.preventDefault();
        if (activeTab === "Design") await doRedo();
      } else if (k === "s" && e.shiftKey) {
        e.preventDefault();
        await saveVersionShortcut();
      } else if (k === "s") {
        e.preventDefault();
        await saveShortcut();
      } else if (k === "n") {
        e.preventDefault();
        await requestNew();
      } else if (k === "g" && e.shiftKey) {
        e.preventDefault();
        if (activeTab === "Design") scene = await core.ungroupOp();
      } else if (k === "g") {
        e.preventDefault();
        if (activeTab === "Design") scene = await core.groupOp();
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  {#if error}
    <div class="error" role="alert">
      <span>Fehler: {error}</span>
      <button onclick={() => (error = null)} aria-label="Fehlermeldung schließen" title="Schließen">×</button>
    </div>
  {/if}

  {#if scene && activeTab !== "Projekt" && activeTab !== "Preview"}
    <Canvas
      {scene}
      tool={activeTab === "Laser" ? "select" : tool}
      {activeShape}
      {insets}
      active={activeTab === "Design"}
      readonlySelection={activeTab === "Laser"}
      fitTrigger={designFitTrigger}
      gridSize={settings?.grid_size_mm ?? 50}
      {ondrawrect}
      {ondrawellipse}
      {ondrawline}
      {ondrawpolyline}
      {ondrawpolygon}
      {onselectat}
      {onselectrect}
      {onmove}
      {onscale}
      {onrotate}
      laserHead={laserHead}
      laserOrigin={laserOrigin}
      laserJobStart={laserJobStart}
      oneditimage={(i) => (editImage = i)}
      onedittext={openTextEdit}
      filletpick={filletPick}
      filletsel={filletCorners}
      onfilletcorner={toggleFilletCorner}
      bridgepick={bridgePick}
      bridgewidth={bridgeWidth}
      onbridgestroke={doBridgeStroke}
      ondragnode={async (sh, n, part, x, y, begin) => (scene = await core.dragNode(sh, n, part, x, y, begin))}
      onhitnodesegment={(x, y, tolerance) => core.hitBezierSegment(x, y, tolerance)}
      onsplitnode={async (sh, seg, t) => (scene = await core.splitNode(sh, seg, t))}
      ondeletenode={async (sh, n) => (scene = await core.deleteNode(sh, n))}
      ontogglenode={async (sh, n) => (scene = await core.toggleNodeSmooth(sh, n))}
      onbezierdone={async (nodes, closed) => (scene = await core.addBezierNodes(nodes, closed))}
    />
  {/if}

  <!-- Laser-Vorschau (ADR 0005): eigener Canvas, gleiche Kamera-Einpassung. -->
  {#if scene && activeTab === "Preview"}
    <PreviewCanvas {scene} {insets} />
  {/if}

  <!-- Zweireihige Topbar: Header + Anordnen bilden die obere Kante des U-Layouts. -->
  {#if settings}
    <div class="header glass">
      <div class="header-main">
        <div class="hleft">
          <span class="brand">
            <img class="brand-logo" src={logoUrl} alt="LuxiFer" height="26" />
            <span class="brand-name">LuxiFer</span>
          </span>
          <!-- Aktuell geladenes Projekt: Name + „•" bei ungespeicherten Änderungen.
               Namenlos ⇒ dezentes „Unbenannt", damit der Slot nie leer wirkt. -->
          <span class="project-tag" title={scene?.project ? scene.project.name : "Noch nicht gespeichert"}>
            <span class="project-name" class:unnamed={!scene?.project}>
              {scene?.project ? scene.project.name : "Unbenannt"}
            </span>
            {#if scene?.dirty}<span class="dirty-dot" title="Ungespeicherte Änderungen">•</span>{/if}
          </span>
          <div class="hgroup">
            <button class="gbtn hbtn" onclick={doUndo} title="Rückgängig (Strg+Z)" aria-label="Rückgängig">
              <Icon name="undo" />
            </button>
            <button class="gbtn hbtn" onclick={doRedo} title="Wiederholen (Strg+Y)" aria-label="Wiederholen">
              <Icon name="redo" />
            </button>
            <button
              class="gbtn hbtn"
              onclick={() => fileInput?.click()}
              title="Datei importieren (PNG, JPG, BMP, WebP, SVG, DXF)"
              aria-label="Bild oder Vektordatei importieren"
            >
              <Icon name="contour" />
            </button>
            <input
              bind:this={fileInput}
              type="file"
              accept=".png,.jpg,.jpeg,.bmp,.webp,.svg,.dxf"
              style="display:none"
              onchange={onImportFile}
            />
          </div>
        </div>

        <div class="tabs">
          {#each ["Projekt", "Design", "Laser", "Monitor", "Preview"] as t}
            <button class="tab" class:active={activeTab === t} onclick={() => (activeTab = t as Tab)}>{t}</button>
          {/each}
        </div>

        <div class="hright">
          <button
            class="gbtn hbtn"
            onclick={() => (showSettings = true)}
            title="Einstellungen"
            aria-label="Einstellungen"
          >
            <Icon name="settings" />
          </button>
        </div>
      </div>

      {#if scene && activeTab === "Design"}
        <div class="header-divider"></div>
        <div class="header-arrange">
          <TransformPanel bbox={selBBox} ontransform={doTransform} />
          <div class="arrange-separator"></div>
          <ArrangePanel
            {selCount}
            onalign={doAlign}
            ondistribute={doDistribute}
            onnest={doNest}
            onnestfill={async (g) => (scene = await core.nestFillOp(g))}
            ongroup={async () => (scene = await core.groupOp())}
            onungroup={async () => (scene = await core.ungroupOp())}
          />
        </div>
      {/if}
    </div>
  {/if}

  <!-- Statische Bedienflaechen: feste Docks statt frei verschiebbarer Panele. -->
  {#if settings && scene && activeTab === "Design"}
    <div class="design-ui">
      <aside class="dock left tools-dock glass">
        <ToolsPanel {tool} onpick={(t) => (tool = t)} onaction={doToolAction} />
      </aside>

      {#if tool === "polygon"}
        <aside class="dock shape-dock glass">
          <ShapesPanel {shapes} {activeShape} onpickshape={pickShape} />
        </aside>
      {/if}

      <aside class="dock right layers-dock glass">
        <LayersPanel layers={sceneLayers} onedit={(i) => (editLayer = i)} ontoggle={toggleLayer} onmovelayer={moveLayer} />
      </aside>

      <section class="dock bottom palette-dock glass">
        <PalettePanel {swatches} onpick={pickColor} />
      </section>
    </div>
  {/if}

  {#if settings && scene && activeTab === "Laser"}
    <aside class="dock left laser-layers-dock glass">
      <LayersPanel layers={sceneLayers} onedit={(i) => (editLayer = i)} ontoggle={toggleLayer} onmovelayer={moveLayer} />
    </aside>

    <aside class="dock right laser-dock glass">
      <LaserPanel
        registry={laserReg}
        actions={laserActions}
        connected={laserConnected}
        hasJob={(scene?.shapes.length ?? 0) > 0}
        onselect={selectLaser}
        onaction={runLaserAction}
        onexport={exportLaser}
        onparamschange={updateLaserJobStart}
        onjog={jogLaser}
        onhome={homeLaser}
        onreadposition={readLaserPosition}
        onopensettings={() => (showLaserSettings = true)}
      />
    </aside>
  {/if}

  {#if settings && scene && activeTab === "Monitor"}
    <aside class="dock right monitor-dock glass">
      <div class="placeholder">Job-Status folgt (Monitor-Reiter).</div>
    </aside>
  {/if}

  <!-- Projekt-Reiter: voller Browser (ADR 0003). -->
  {#if settings && scene && activeTab === "Projekt"}
    <ProjectBrowser
      project={scene.project}
      {saveMode}
      onsave={saveWithMeta}
      onopen={requestOpen}
      onopenversion={openVersion}
      ondeleteversion={deleteVersion}
      onnew={requestNew}
      ondeleted={() => { scene && (scene.project = null); }}
      onclosesavemode={() => (saveMode = false)}
    />
  {/if}

  <!-- Layer-Dialog -->
  {#if scene && editLayer !== null && scene.layers[editLayer]}
    <LayerDialog
      layer={scene.layers[editLayer]}
      onsave={saveLayer}
      oncancel={() => (editLayer = null)}
    />
  {/if}

  <!-- Geometrie-Werkzeuge (Boolean/Fillet/Offset/Muster) aus der Toolbar -->
  {#if geoTool !== null}
    <GeoToolDialog
      kind={geoTool}
      {selCount}
      onboolean={(op) => { doBoolean(op); geoTool = null; }}
      onfillet={(r) => { doFillet(r); geoTool = null; }}
      onfilletpick={(r) => { filletRadius = r; filletCorners = []; filletPick = true; geoTool = null; }}
      onoffset={(d) => { doOffset(d); geoTool = null; }}
      onpattern={doPatternFill}
      onclose={() => (geoTool = null)}
    />
  {/if}

  <!-- Haltesteg-Modus: schwebende Leiste -->
  {#if bridgePick}
    <div class="fillet-bar glass">
      <span>Haltesteg: Linie über die Kontur ziehen</span>
      <label>Breite <input type="number" step="0.5" min="0.2" bind:value={bridgeWidth} /> mm</label>
      <button class="mini" onclick={() => (bridgePick = false)}>Fertig</button>
    </div>
  {/if}

  <!-- Fillet-Ecken-Modus: schwebende Leiste -->
  {#if filletPick}
    <div class="fillet-bar glass">
      <span>Ecken anklicken ({filletCorners.length} gewählt)</span>
      <label>Radius <input type="number" step="0.5" min="0.1" bind:value={filletRadius} /></label>
      <button class="mini primary" disabled={filletCorners.length === 0} onclick={applyFilletCorners}>Anwenden</button>
      <button class="mini" onclick={() => { filletPick = false; filletCorners = []; }}>Abbrechen</button>
    </div>
  {/if}

  <!-- Text-Werkzeug (Text→Pfad) -->
  {#if textOpen}
    <TextDialog
      initial={textEditMeta}
      oninsert={doInsertText}
      onclose={() => { textOpen = false; textEdit = null; }}
    />
  {/if}

  <!-- Bild-Editor (ADR 0004): Doppelklick auf ein Bild öffnet ihn. Die Bedingung
       hängt am stabilen Shape-Index (nicht am abgeleiteten Objekt), damit der
       Dialog beim Live-Update der Parameter nicht neu montiert wird. -->
  {#if editImage !== null && editImageShape}
    {#key editImage}
      <ImageEditor
        asset={editImageShape.asset}
        params={editImageShape.params}
        onapply={applyImageParams}
        onclose={() => (editImage = null)}
        onvectorize={doTraceImage}
      />
    {/key}
  {/if}

  {#if status}
    <div class="status">{status}</div>
  {/if}

  <!-- Start-Toast: zuletzt geoeffnetes Projekt anbieten (ADR 0003 §3). -->
  {#if startToast}
    <div class="toast glass">
      <span>Zuletzt: <strong>{startToast}</strong></span>
      <div class="toast-actions">
        <button class="primary" onclick={toastOpen}>Öffnen</button>
        <button onclick={() => (startToast = null)}>Verwerfen</button>
      </div>
    </div>
  {/if}

  <!-- Unsaved-Guard: ungesicherte Aenderungen bei Neu/Oeffnen (ADR 0003 §3). -->
  {#if pendingAction}
    <div class="backdrop" role="button" tabindex="-1"
      onclick={guardCancel}
      onkeydown={(e) => e.key === "Escape" && guardCancel()}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="guard glass" onclick={(e) => e.stopPropagation()}>
        <h3>Ungespeicherte Änderungen</h3>
        <p>Der aktuelle Entwurf ist nicht gespeichert. Was möchtest du tun?</p>
        <div class="guard-actions">
          <button class="danger" onclick={guardDiscard}>Verwerfen</button>
          <button class="primary" onclick={guardSave}>
            {scene?.project ? "Speichern" : "Speichern unter…"}
          </button>
          <button onclick={guardCancel}>Abbrechen</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Laser-Verwaltung als Schnellzugriff aus dem Laserpanel (ADR 0007) -->
  {#if showLaserSettings}
    <LaserSettings
      registry={laserReg}
      onsave={saveLaser}
      ondelete={deleteLaser}
      onclose={() => (showLaserSettings = false)}
    />
  {/if}

  <!-- Zentrales Einstellungs-Modal (Zahnrad): Laser + Oberfläche -->
  {#if showSettings}
    <SettingsModal
      registry={laserReg}
      settings={settings}
      version={appVer}
      onsave={saveLaser}
      ondelete={deleteLaser}
      onsavesettings={persist}
      onclose={() => (showSettings = false)}
    />
  {/if}
</main>

<style>
  main {
    position: absolute;
    inset: 0;
    /* 6px oben + 34px Hauptzeile + 12px Trenner + 42px Kontextzeile +
       8px unten. Diese Kante teilen Canvas, Lineal und beide Seitendocks. */
    --topbar-h: 102px;
    /* Einreihige Topbar (Laser/Monitor): nur header-main, ohne Anordnen-Zeile. */
    --topbar-h1: 48px;
    --left-dock-w: 96px;
    --right-dock-w: 316px;
  }
  /* Zweireihige Topbar: oben App/Reiter, unten kontextuelle Anordnen-Werkzeuge. */
  .header {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 6px 10px 8px;
    border-radius: 0;
    z-index: 50;
  }
  .header-main {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 10px;
    min-height: 34px;
  }
  .header-divider {
    height: 1px;
    margin: 5px -2px 6px;
    background: linear-gradient(
      90deg,
      transparent,
      var(--border) 10%,
      var(--border) 90%,
      transparent
    );
  }
  .header-arrange {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .header-arrange::-webkit-scrollbar { display: none; }
  .arrange-separator {
    width: 1px;
    height: 40px;
    margin: 0 var(--sp-2);
    background: var(--border);
    flex: 0 0 1px;
  }
  .hleft {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-self: start;
  }
  .hright {
    display: flex;
    align-items: center;
    justify-self: end;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .brand-logo {
    display: block;
    /* Querformatiges Logo (3:2): Breite folgt der Höhe, damit nichts gestaucht
       wird. object-fit: contain hält es zusätzlich proportional im Slot. */
    width: auto;
    height: 26px;
    object-fit: contain;
    filter: drop-shadow(0 0 5px hsl(var(--accent-h) var(--accent-s) var(--accent-l) / 0.45));
  }
  .brand-name {
    font-weight: 700;
    letter-spacing: 0.5px;
    font-size: var(--fs-lg);
  }
  /* Projekt-Anzeige: durch einen Trenner von der Marke abgesetzt. */
  .project-tag {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-left: var(--sp-3);
    margin-left: var(--sp-1);
    border-left: 1px solid var(--border-soft);
    min-width: 0;
  }
  .project-name {
    font-size: var(--fs-md);
    color: var(--text);
    max-width: 22ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .project-name.unnamed {
    color: var(--muted);
    font-style: italic;
  }
  .dirty-dot {
    color: var(--accent);
    font-size: 18px;
    line-height: 1;
  }
  .hgroup {
    display: flex;
    gap: var(--sp-1);
    padding-left: var(--sp-3);
    border-left: 1px solid var(--border-soft);
  }
  .hbtn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    color: var(--muted);
  }
  .hbtn:hover { color: var(--text); }
  .tabs {
    display: flex;
    gap: 4px;
    justify-self: center;
  }
  .tab {
    background: transparent;
    color: var(--muted);
    border: none;
    border-radius: var(--r-md);
    padding: var(--sp-2) var(--sp-4);
    cursor: pointer;
    font-size: var(--fs-md);
    font-weight: 500;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.active {
    background: linear-gradient(
      180deg,
      hsl(var(--accent-h) var(--accent-s) calc(var(--accent-l) + 8%)),
      var(--accent)
    );
    color: white;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.3),
      0 0 14px -3px hsl(var(--accent-h) var(--accent-s) var(--accent-l) / 0.6);
  }
  .dock {
    position: absolute;
    z-index: 20;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .dock.left {
    left: 0;
    top: var(--topbar-h);
    bottom: 86px;
    width: var(--left-dock-w);
    border-radius: 0 0 10px 0;
    border-top: 0;
  }
  .shape-dock {
    left: 108px;
    top: var(--topbar-h);
    width: 176px;
    max-height: min(420px, calc(100vh - 190px));
    padding: 8px;
  }
  .dock.right {
    top: var(--topbar-h);
    right: 0;
    bottom: 12px;
    width: var(--right-dock-w);
    border-radius: 0 0 0 10px;
    border-top: 0;
  }
  .dock.bottom {
    left: 50%;
    bottom: 14px;
    width: clamp(320px, calc(100vw - 460px), 520px);
    min-height: 64px;
    transform: translateX(-50%);
  }
  .tools-dock,
  .layers-dock,
  .laser-layers-dock,
  .laser-dock,
  .monitor-dock {
    padding: 8px;
  }
  .palette-dock {
    padding: 8px;
  }
  .tools-dock {
    overflow-y: auto;
  }
  .layers-dock,
  .laser-layers-dock,
  .laser-dock,
  .monitor-dock {
    min-height: 0;
  }
  /* Laser-Docks bündig unter der einreihigen Topbar, gleiche Optik wie die
     Design-Docks (gerundete Innenecke, ohne Oberkante). Der linke Ebenen-Dock
     spiegelt den rechten Design-Dock. Doppelte Klasse, damit die Breite/Position
     .dock.left bzw. .dock.right überstimmt (gleiche Spezifität sonst). */
  .dock.left.laser-layers-dock {
    top: var(--topbar-h1);
    bottom: 12px;
    width: var(--right-dock-w);
    border-radius: 0 0 10px 0;
    border-top: 0;
    border-left: 0;
  }
  .dock.right.laser-dock,
  .monitor-dock {
    top: var(--topbar-h1);
  }
  .placeholder {
    color: var(--muted);
    font-size: 13px;
    padding: 8px;
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
    z-index: 90;
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid #e5645d55;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.3);
  }
  .error button {
    border: 0;
    background: transparent;
    color: inherit;
    padding: 0 2px;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }
  .status {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    background: #1a2b22;
    color: #3fb27f;
    padding: 8px 16px;
    border-radius: 8px;
    z-index: 90;
    border: 1px solid #3fb27f55;
  }
  /* Start-Toast oben rechts (zuletzt geoeffnetes Projekt). */
  .toast {
    position: absolute;
    top: 64px;
    right: 16px;
    z-index: 95;
    padding: 12px 14px;
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 280px;
  }
  .toast-actions { display: flex; gap: 8px; justify-content: flex-end; }
  .toast button { padding: 5px 12px; font-size: 13px; }
  /* Unsaved-Guard-Dialog. */
  .guard {
    width: min(420px, 90%);
    padding: 20px 22px;
    border-radius: 12px;
  }
  .guard h3 { margin: 0 0 8px; }
  .guard p { margin: 0 0 16px; color: var(--muted); font-size: 14px; }
  .guard-actions { display: flex; gap: 8px; justify-content: flex-end; flex-wrap: wrap; }
  .guard .danger { background: #6b2b2b; color: #ffb4b4; }
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  button {
    background: var(--btn);
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 6px 10px;
    cursor: pointer;
    transition: filter 0.14s;
  }
  button:hover {
    filter: brightness(1.15);
  }
  .primary {
    background: var(--accent);
    color: white;
  }
  .fillet-bar {
    position: absolute;
    top: 70px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    border-radius: 10px;
    font-size: 13px;
    color: var(--text);
    z-index: 60;
  }
  .fillet-bar label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .fillet-bar input {
    width: 60px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 4px 6px;
  }
  .fillet-bar .primary {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
</style>
