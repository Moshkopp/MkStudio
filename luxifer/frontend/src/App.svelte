<script lang="ts">
  import Canvas from "./lib/Canvas.svelte";
  import LayerDialog from "./lib/LayerDialog.svelte";
  import LaserPanel from "./lib/LaserPanel.svelte";
  import PanelHost from "./lib/PanelHost.svelte";
  import ToolsPanel from "./lib/ToolsPanel.svelte";
  import LayersPanel from "./lib/LayersPanel.svelte";
  import PalettePanel from "./lib/PalettePanel.svelte";
  import ShapesPanel from "./lib/ShapesPanel.svelte";
  import ArrangePanel from "./lib/ArrangePanel.svelte";
  import EditFlyout from "./lib/EditFlyout.svelte";
  import Icon from "./lib/Icon.svelte";
  import logoUrl from "./assets/logo.png";
  import * as core from "./lib/core";
  import type {
    Scene,
    LayerParams,
    UiSettings,
    Tab,
    PanelKind,
    PanelRect,
    PanelPlacement,
  } from "./lib/core";
  import { applyTheme } from "./lib/theme";

  type Tool = "select" | "rect" | "ellipse" | "line" | "polyline" | "polygon";

  let scene = $state<Scene | null>(null);
  let tool = $state<Tool>("rect");
  let swatches = $state<[number, number, number][]>([]);
  // Formen-Katalog (datengetrieben aus dem Core) + aktuell gewaehlte Form.
  let shapes = $state<core.ShapeInfo[]>([]);
  let activeShape = $state("hex");
  let error = $state<string | null>(null);
  let editLayer = $state<number | null>(null);
  let gcode = $state<string | null>(null);
  let status = $state<string | null>(null);

  // --- GUI-Settings (Panel-System, ADR 0002) --------------------------------
  let settings = $state<UiSettings | null>(null);
  let activeTab = $state<Tab>("Design");
  // Editier-Modus ist fluechtig (nicht gespeichert).
  let editing = $state(false);
  let lockHover = $state(false);

  async function load() {
    try {
      scene = await core.getScene();
      swatches = await core.swatchColors();
      shapes = await core.shapeCatalog();
      settings = await core.getUiSettings();
      applyTheme(settings.theme);
    } catch (e) {
      error = String(e);
    }
  }
  load();

  // Aktuelles Reiter-Layout (Panele + Positionen).
  const layout = $derived(settings?.layouts.find((l) => l.tab === activeTab));
  const panels = $derived<PanelPlacement[]>(layout?.panels ?? []);
  const visibleKinds = $derived<PanelKind[]>(panels.map((p) => p.kind));
  // Panele, die nur bei Bedarf sichtbar sind (im Editier-Modus zeigt der Host
  // sie trotzdem). Formen erscheint nur, wenn das Polygon-Werkzeug aktiv ist.
  const hiddenPanels = $derived<PanelKind[]>(
    tool === "polygon" ? [] : ["Formen"],
  );

  // Fenstergroesse (reaktiv), damit sich die Bett-Einpassung an Resize anpasst.
  let winW = $state(0);
  let winH = $state(0);

  // Freie Raender (px) fuer die Bett-Einpassung im Canvas: Header oben fest,
  // seitlich/unten aus den Panel-Rects grob geschaetzt. Ein Panel zaehlt fuer
  // eine Kante, wenn es dort klebt (z. B. x≈0 -> linker Rand). So landet das
  // Bett im tatsaechlich freien Bereich, ohne dass Canvas die Panel-Logik kennt.
  const HEADER_PX = 56;
  const insets = $derived.by(() => {
    const ins = { top: HEADER_PX, right: 0, bottom: 0, left: 0 };
    if (!winW || !winH) return ins;
    for (const p of panels) {
      const { x, y, w, h } = p.rect;
      // Panel-Rects sind Bruchteile: Breite gegen Fensterbreite, Hoehe gegen
      // Fensterhoehe (PanelHost rendert prozentual pro Achse).
      const wpx = w * winW, hpx = h * winH;
      if (x <= 0.02) ins.left = Math.max(ins.left, x * winW + wpx);
      if (x + w >= 0.98) ins.right = Math.max(ins.right, wpx);
      if (y <= 0.02) ins.top = Math.max(ins.top, HEADER_PX, y * winH + hpx);
      if (y + h >= 0.98) ins.bottom = Math.max(ins.bottom, hpx);
    }
    return ins;
  });

  // Settings lokal sofort anwenden (fluessig), Persistieren auf Platte
  // gedrosselt. Beim Panel-Ziehen darf NICHT jede Mausbewegung eine JSON auf
  // die Platte schreiben — das war die Ruckel-Ursache im Editier-Modus.
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleSave(next: UiSettings) {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      saveTimer = null;
      try {
        settings = await core.saveUiSettings($state.snapshot(next));
        applyTheme(settings.theme);
      } catch (e) {
        error = String(e);
      }
    }, 250);
  }

  // Direktes, sofortiges Speichern (z. B. Reset) ohne Drosselung.
  async function persist(next: UiSettings) {
    settings = next;
    applyTheme(next.theme);
    try {
      settings = await core.saveUiSettings(next);
      applyTheme(settings.theme);
    } catch (e) {
      error = String(e);
    }
  }

  // Ein Panel-Rect im aktuellen Reiter aendern (Drag/Resize aus dem Host).
  // Lokal sofort setzen (fluessige Vorschau), Speichern gedrosselt.
  function changeRect(i: number, rect: PanelRect) {
    if (!settings) return;
    const next: UiSettings = structuredClone($state.snapshot(settings));
    const l = next.layouts.find((l) => l.tab === activeTab);
    if (l && l.panels[i]) {
      l.panels[i].rect = rect;
      settings = next; // sofort sichtbar
      scheduleSave(next); // Platte gedrosselt
    }
  }

  // Panel im aktuellen Reiter ein-/ausblenden.
  function togglePanel(kind: PanelKind) {
    if (!settings) return;
    const next: UiSettings = structuredClone($state.snapshot(settings));
    const l = next.layouts.find((l) => l.tab === activeTab);
    if (!l) return;
    const idx = l.panels.findIndex((p) => p.kind === kind);
    if (idx >= 0) {
      l.panels.splice(idx, 1);
    } else {
      // Neu einblenden: mittig, moderate Groesse.
      l.panels.push({ kind, rect: { x: 0.35, y: 0.35, w: 0.3, h: 0.3, z: 0 } });
    }
    persist(next);
  }

  // Reiter auf sein Standard-Layout zuruecksetzen (Core-Command, ADR §2).
  async function resetTab() {
    try {
      settings = await core.resetTab(activeTab);
      applyTheme(settings.theme);
    } catch (e) {
      error = String(e);
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
    scene = await core.addPolyline(pts, closed);
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
  // Sofort-Befehle aus der Werkzeugleiste (Spiegeln). Wirken auf die Auswahl.
  async function doToolAction(a: "mirror_h" | "mirror_v") {
    scene = await core.mirror(a === "mirror_h" ? "h" : "v");
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
  async function pingRuida(ip: string): Promise<boolean> {
    try {
      const ok = await core.ruidaPing(ip);
      status = ok ? `Verbunden mit ${ip}` : `Keine Antwort von ${ip}`;
      setTimeout(() => (status = null), 3000);
      return ok;
    } catch (e) {
      error = String(e);
      return false;
    }
  }
  async function sendRuida(ip: string) {
    try {
      status = await core.ruidaSend(ip);
      setTimeout(() => (status = null), 4000);
    } catch (e) {
      error = String(e);
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
      if (selCount > 0) {
        e.preventDefault();
        await doDelete();
      }
      return;
    }
    // Strg+Z / Strg+Y (bzw. Strg+Shift+Z) fuer Undo/Redo.
    if ((e.ctrlKey || e.metaKey) && !e.altKey) {
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) {
        e.preventDefault();
        await doUndo();
      } else if (k === "y" || (k === "z" && e.shiftKey)) {
        e.preventDefault();
        await doRedo();
      }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} bind:innerWidth={winW} bind:innerHeight={winH} />

<main>
  {#if error}
    <div class="error">Fehler: {error}</div>
  {/if}

  {#if scene}
    <Canvas
      {scene}
      {tool}
      {activeShape}
      {insets}
      {ondrawrect}
      {ondrawellipse}
      {ondrawline}
      {ondrawpolyline}
      {ondrawpolygon}
      {onselectat}
      {onselectrect}
      {onmove}
      {onscale}
    />
  {/if}

  <!-- Header über volle Breite: links Logo + Name + Undo/Redo, Reiter mittig
       zentriert, rechts Zahnrad (Settings/Editier-Modus). -->
  {#if settings}
    <div class="header glass">
      <div class="hleft">
        <span class="brand">
          <img class="brand-logo" src={logoUrl} alt="LuxiFer" width="26" height="26" />
          <span class="brand-name">LuxiFer</span>
        </span>
        <div class="hgroup">
          <button class="gbtn hbtn" onclick={doUndo} title="Rückgängig (Strg+Z)" aria-label="Rückgängig">
            <Icon name="undo" />
          </button>
          <button class="gbtn hbtn" onclick={doRedo} title="Wiederholen (Strg+Y)" aria-label="Wiederholen">
            <Icon name="redo" />
          </button>
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
          class:active={editing}
          onclick={() => (editing = !editing)}
          title="Einstellungen / Oberfläche bearbeiten"
          aria-label="Einstellungen"
        >
          <Icon name="settings" />
        </button>
      </div>
    </div>
  {/if}

  <!-- Panel-Host: rendert die Panele des aktiven Reiters aus den Settings -->
  {#if settings && scene}
    <PanelHost {panels} {editing} hidden={hiddenPanels} onchange={changeRect}>
      {#snippet panel(p: PanelPlacement)}
        {#if p.kind === "Werkzeuge"}
          <ToolsPanel {tool} onpick={(t) => (tool = t)} onaction={doToolAction} />
        {:else if p.kind === "Ebenen"}
          <LayersPanel layers={sceneLayers} onedit={(i) => (editLayer = i)} ontoggle={toggleLayer} />
        {:else if p.kind === "Farbpalette"}
          <PalettePanel {swatches} onpick={pickColor} />
        {:else if p.kind === "Formen"}
          <ShapesPanel {shapes} {activeShape} onpickshape={pickShape} />
        {:else if p.kind === "Anordnen"}
          <ArrangePanel {selCount} onalign={doAlign} ondistribute={doDistribute} />
        {:else if p.kind === "Laser"}
          <LaserPanel ongenerate={generateGcode} onping={pingRuida} onsend={sendRuida} />
        {:else if p.kind === "JobStatus"}
          <div class="placeholder">Job-Status folgt (Monitor-Reiter).</div>
        {/if}
      {/snippet}
    </PanelHost>
  {/if}

  <!-- Noch leere Reiter (Projekt/Preview): dezenter Hinweis mittig. -->
  {#if settings && panels.length === 0}
    <div class="empty-tab">
      {#if activeTab === "Projekt"}
        Projektverwaltung folgt als eigener Meilenstein.
      {:else if activeTab === "Preview"}
        Laser-Vorschau folgt als eigener Meilenstein.
      {:else}
        Dieser Reiter ist noch leer.
      {/if}
    </div>
  {/if}

  <!-- Layer-Dialog -->
  {#if scene && editLayer !== null && scene.layers[editLayer]}
    <LayerDialog
      layer={scene.layers[editLayer]}
      onsave={saveLayer}
      oncancel={() => (editLayer = null)}
    />
  {/if}

  <!-- Verstecktes Schloss unten links (Editier-Modus, ADR 0002 §5) -->
  <button
    class="lock"
    class:show={lockHover || editing}
    class:on={editing}
    onmouseenter={() => (lockHover = true)}
    onmouseleave={() => (lockHover = false)}
    onclick={() => (editing = !editing)}
    title={editing ? "Editier-Modus verlassen" : "Oberfläche bearbeiten"}
    aria-label="Editier-Modus umschalten"
  ><Icon name="lock" size={15} /></button>

  <!-- Theming-/Layout-Flyout im Editier-Modus -->
  {#if editing && settings}
    <EditFlyout
      {settings}
      tab={activeTab}
      visiblePanels={visibleKinds}
      onchange={persist}
      ontogglepanel={togglePanel}
      onreset={resetTab}
      onclose={() => (editing = false)}
    />
  {/if}

  {#if status}
    <div class="status">{status}</div>
  {/if}

  <!-- G-Code-Overlay -->
  {#if gcode !== null}
    <div
      class="backdrop"
      onclick={() => (gcode = null)}
      onkeydown={(e) => e.key === "Escape" && (gcode = null)}
      role="button"
      tabindex="-1"
    >
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="gcode glass" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
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
  /* Header über die volle Breite; drei Zonen (links | Reiter mittig | rechts).
     Das mittlere Grid-Fach ist zentriert, egal wie breit links/rechts sind. */
  .header {
    position: absolute;
    left: 8px;
    right: 8px;
    top: 8px;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    z-index: 50;
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
    width: 26px;
    height: 26px;
    object-fit: contain;
    filter: drop-shadow(0 0 5px hsl(var(--accent-h) var(--accent-s) var(--accent-l) / 0.45));
  }
  .brand-name {
    font-weight: 700;
    letter-spacing: 0.5px;
    font-size: 15px;
  }
  .hgroup {
    display: flex;
    gap: 4px;
    padding-left: 10px;
    border-left: 1px solid var(--border);
  }
  .hbtn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    justify-self: center;
  }
  .tab {
    background: transparent;
    color: var(--muted);
    border: none;
    border-radius: 8px;
    padding: 6px 16px;
    cursor: pointer;
    font-size: 13px;
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
  .placeholder {
    color: var(--muted);
    font-size: 13px;
    padding: 8px;
  }
  .empty-tab {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    color: var(--muted);
    font-size: 14px;
    text-align: center;
    pointer-events: none;
  }
  .lock {
    position: absolute;
    left: 10px;
    bottom: 10px;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    border: none;
    background: rgba(28, 30, 34, 0.6);
    color: var(--text);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.2s;
    z-index: 70;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .lock.show {
    opacity: 0.85;
  }
  .lock.on {
    background: var(--accent);
    opacity: 1;
    box-shadow: 0 0 18px -2px hsl(var(--accent-h) var(--accent-s) var(--accent-l) / 0.7);
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
    width: min(600px, 90%);
    max-height: 80%;
    display: flex;
    flex-direction: column;
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
</style>
