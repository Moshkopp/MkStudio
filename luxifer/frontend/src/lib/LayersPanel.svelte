<script lang="ts">
  // Ebenenliste (Farbe = Layer). Jede Ebene ist eine Glaskachel mit ihren
  // Parametern und drei Schaltern. Doppelklick oeffnet den Parameter-Dialog.
  import { rgb } from "./core";
  import type { Layer, LayerToggle } from "./core";
  let {
    layers,
    onedit,
    ontoggle,
    onmovelayer,
  }: {
    layers: Layer[];
    onedit: (i: number) => void;
    ontoggle: (i: number, field: LayerToggle) => void;
    // Verschiebt einen Layer in der Brenn-Reihenfolge (ADR 0005 §0).
    onmovelayer?: (from: number, to: number) => void;
  } = $props();

  function toggle(e: Event, i: number, field: LayerToggle) {
    e.stopPropagation();
    ontoggle(i, field);
  }

  // ---- Drag & Drop: Brenn-Reihenfolge umsortieren (ADR 0005 §0) ------------
  // Index oben = zuerst gebrannt (= Core-Reihenfolge, Index 0). Wir ziehen eine
  // Kachel und legen sie vor/nach einer anderen ab; der Core remappt layer_id.
  let dragFrom = $state<number | null>(null);
  // Ziel-Einfuegepunkt: Index, VOR dem eingefuegt wird (0..layers.length).
  let dropAt = $state<number | null>(null);

  function onDragStart(e: DragEvent, i: number) {
    dragFrom = i;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // Firefox verlangt gesetzte Daten, sonst startet der Drag nicht.
      e.dataTransfer.setData("text/plain", String(i));
    }
  }

  // Bestimmt aus der Mausposition, ob vor oder nach der Kachel eingefuegt wird.
  function onDragOver(e: DragEvent, i: number) {
    if (dragFrom === null) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    const el = e.currentTarget as HTMLElement;
    const r = el.getBoundingClientRect();
    const after = e.clientY > r.top + r.height / 2;
    dropAt = after ? i + 1 : i;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    const from = dragFrom;
    const at = dropAt;
    dragFrom = null;
    dropAt = null;
    if (from === null || at === null) return;
    // Einfuegepunkt `at` in Ziel-Index umrechnen: Entfernt man erst `from`,
    // verschiebt sich alles danach um eins.
    let to = at > from ? at - 1 : at;
    if (to === from) return; // keine echte Bewegung
    onmovelayer?.(from, to);
  }

  function onDragEnd() {
    dragFrom = null;
    dropAt = null;
  }
</script>

<div class="layers">
  <span class="label">Ebenen · oben = zuerst · ziehen sortiert</span>
  {#each layers as l, i}
    <div
      class="layer"
      class:disabled={!l.enabled}
      class:dragging={dragFrom === i}
      class:drop-before={dropAt === i}
      class:drop-after={dropAt === i + 1}
      style="--lc: {rgb(l.color)}"
      draggable={onmovelayer ? true : undefined}
      ondragstart={(e) => onDragStart(e, i)}
      ondragover={(e) => onDragOver(e, i)}
      ondrop={onDrop}
      ondragend={onDragEnd}
      ondblclick={() => onedit(i)}
      onkeydown={(e) => e.key === "Enter" && onedit(i)}
      role="button"
      tabindex="0"
    >
      <div class="top">
        {#if onmovelayer}
          <span class="grip" title="Zum Umsortieren ziehen" aria-hidden="true">⠿</span>
        {/if}
        <span class="name">{l.name}</span>
        <span class="mode">{l.mode}</span>
      </div>
      <div class="params">
        <span title="Geschwindigkeit">⏵ {l.speed_mm_s} mm/s</span>
        <span title="Leistung min–max">⚡ {l.min_power_pct}–{l.power_pct}%</span>
      </div>
      <div class="switches">
        <button
          class="sw"
          class:on={l.air_assist}
          title="Air Assist"
          onclick={(e) => toggle(e, i, "air_assist")}
        >💨 <span class="sl">Luft</span></button>
        <button
          class="sw"
          class:on={l.enabled}
          title="Im Job brennen"
          onclick={(e) => toggle(e, i, "enabled")}
        >🔥 <span class="sl">Aktiv</span></button>
        <button
          class="sw"
          class:on={l.visible}
          title="Objekte anzeigen"
          onclick={(e) => toggle(e, i, "visible")}
        >{l.visible ? "👁" : "◠"} <span class="sl">Zeigen</span></button>
      </div>
    </div>
  {/each}
  {#if layers.length === 0}
    <div class="muted">— noch leer —</div>
  {/if}
</div>

<style>
  .layers {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .label {
    font-size: 11px;
    letter-spacing: 1px;
    color: var(--muted);
    text-transform: uppercase;
  }
  /* Jede Ebene ist eine Frostglas-Kachel. Die Layer-Farbe (--lc) faerbt die
     linke und rechte Kante und zieht sich als dezenter Waschgang durch die
     ganze Flaeche — die Mitte traegt die Parameter. */
  .layer {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px 11px;
    cursor: pointer;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-left: 3px solid var(--lc);
    border-right: 3px solid var(--lc);
    background:
      linear-gradient(
        90deg,
        color-mix(in srgb, var(--lc) 24%, transparent),
        color-mix(in srgb, var(--lc) 7%, transparent) 45%,
        color-mix(in srgb, var(--lc) 7%, transparent) 55%,
        color-mix(in srgb, var(--lc) 24%, transparent)
      ),
      linear-gradient(180deg, rgba(255, 255, 255, 0.06), rgba(255, 255, 255, 0.02));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.1),
      0 1px 3px rgba(0, 0, 0, 0.28);
    transition:
      box-shadow 0.14s ease,
      opacity 0.14s ease,
      transform 0.08s ease;
  }
  .layer:hover {
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.14),
      0 0 12px -3px color-mix(in srgb, var(--lc) 70%, transparent),
      0 2px 6px rgba(0, 0, 0, 0.3);
  }
  .layer:active {
    transform: translateY(1px);
  }
  /* Deaktivierter Layer (wird nicht gebrannt): gedimmt. */
  .layer.disabled {
    opacity: 0.5;
  }
  /* Waehrend des Ziehens: die gegriffene Kachel ausblassen. */
  .layer.dragging {
    opacity: 0.4;
  }
  /* Einfuege-Indikator: leuchtende Linie ober-/unterhalb der Ziel-Kachel. */
  .layer.drop-before {
    box-shadow: 0 -2px 0 -0px var(--accent, #6ea8ff), inset 0 1px 0 rgba(255, 255, 255, 0.1);
  }
  .layer.drop-after {
    box-shadow: 0 2px 0 -0px var(--accent, #6ea8ff), inset 0 1px 0 rgba(255, 255, 255, 0.1);
  }

  .top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  /* Griff zum Ziehen: dezent, faerbt beim Hover in der Layer-Farbe. */
  .grip {
    cursor: grab;
    color: var(--muted);
    font-size: 13px;
    line-height: 1;
    letter-spacing: -2px;
    align-self: center;
    opacity: 0.6;
    user-select: none;
  }
  .grip:hover {
    opacity: 1;
    color: var(--lc);
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mode {
    font-size: 10px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--muted);
    padding: 1px 6px;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.25);
    flex-shrink: 0;
  }
  .params {
    display: flex;
    gap: 12px;
    font-size: 11px;
    color: var(--muted);
  }
  .switches {
    display: flex;
    gap: 5px;
    margin-top: 1px;
  }
  /* Schalter: aus = transluzent-grau, an = leuchtet in der Layer-Farbe. */
  .sw {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 4px 2px;
    font-size: 12px;
    border-radius: 7px;
    cursor: pointer;
    color: var(--muted);
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.07);
    transition:
      background 0.14s ease,
      color 0.14s ease,
      box-shadow 0.14s ease;
  }
  .sw .sl {
    font-size: 10px;
  }
  .sw:hover {
    border-color: rgba(255, 255, 255, 0.18);
  }
  .sw.on {
    color: var(--text);
    background: color-mix(in srgb, var(--lc) 30%, transparent);
    border-color: color-mix(in srgb, var(--lc) 60%, transparent);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.18),
      0 0 10px -3px color-mix(in srgb, var(--lc) 70%, transparent);
  }
  .muted {
    color: var(--muted);
  }
</style>
