<script lang="ts">
  // Container fuer ein Reiter-Layout (ADR 0002 §1). Positioniert Panele aus
  // Bruchteil-Rects (0…1) absolut ueber dem Canvas. Panele duerfen frei
  // ueberlappen (keine Kollisionslogik), z steuert die Stapel-Reihenfolge.
  //
  // Im Editier-Modus bekommt jedes Panel eine Bounding-Box wie eine selektierte
  // Shape im Canvas: duenner Rahmen plus Greifpunkte. Verschieben durch Ziehen
  // der Flaeche, Skalieren ueber die Ecken/Kanten — frei, ohne Raster-Snap.
  import type { PanelPlacement, PanelRect, PanelKind } from "./core";
  import type { Snippet } from "svelte";

  let {
    panels,
    editing,
    hidden,
    panel,
    onchange,
  }: {
    panels: PanelPlacement[];
    editing: boolean;
    // Panel-Arten, die im Normalbetrieb NICHT gerendert werden (aber im Layout
    // bleiben — Position gemerkt). Im Editier-Modus werden sie trotzdem
    // gezeigt, damit man sie positionieren kann.
    hidden?: PanelKind[];
    panel: Snippet<[PanelPlacement]>;
    onchange: (i: number, rect: PanelRect) => void;
  } = $props();

  // Wird ein Panel aktuell versteckt? (Im Editier-Modus nie — dort alles sichtbar.)
  const isHidden = (kind: PanelKind) => !editing && !!hidden?.includes(kind);

  let host = $state<HTMLDivElement>();

  // --- Drag/Resize (frei, ohne Snap) ----------------------------------------
  // Griff-Richtungen: "move" verschiebt, die uebrigen skalieren an der
  // jeweiligen Kante/Ecke.
  type Handle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
  let drag = $state<{
    handle: Handle;
    index: number;
    startRect: PanelRect;
    px0: number;
    py0: number;
  } | null>(null);

  function hostSize(): { w: number; h: number } {
    const r = host?.getBoundingClientRect();
    return { w: r?.width ?? 1, h: r?.height ?? 1 };
  }

  function startDrag(e: PointerEvent, handle: Handle, index: number) {
    if (!editing) return;
    e.preventDefault();
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    drag = {
      handle,
      index,
      startRect: { ...panels[index].rect },
      px0: e.clientX,
      py0: e.clientY,
    };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const { w, h } = hostSize();
    const dx = (e.clientX - drag.px0) / w;
    const dy = (e.clientY - drag.py0) / h;
    const s = drag.startRect;
    let r: PanelRect = { ...s };

    if (drag.handle === "move") {
      r.x = s.x + dx;
      r.y = s.y + dy;
    } else {
      // Kanten anfassen: nord/west verschieben Ursprung und aendern Groesse,
      // sued/ost aendern nur die Groesse.
      if (drag.handle.includes("w")) {
        r.x = s.x + dx;
        r.w = s.w - dx;
      }
      if (drag.handle.includes("e")) {
        r.w = s.w + dx;
      }
      if (drag.handle.includes("n")) {
        r.y = s.y + dy;
        r.h = s.h - dy;
      }
      if (drag.handle.includes("s")) {
        r.h = s.h + dy;
      }
    }
    onchange(drag.index, clampRect(r));
  }

  function endDrag(e: PointerEvent) {
    if (!drag) return;
    (e.target as HTMLElement).releasePointerCapture?.(e.pointerId);
    drag = null;
  }

  // Haelt das Panel im Fenster; verhindert negative/zu kleine Groessen. Beim
  // Ziehen an einer Nord/West-Kante darf der Ursprung nicht ueber die
  // gegenueberliegende Kante hinauswandern.
  const MIN = 0.04;
  function clampRect(r: PanelRect): PanelRect {
    let { x, y, w, h } = r;
    if (w < MIN) w = MIN;
    if (h < MIN) h = MIN;
    if (x < 0) x = 0;
    if (y < 0) y = 0;
    if (x + w > 1) {
      // Bei Move zurueckschieben, bei Resize begrenzen.
      if (drag?.handle === "move") x = 1 - w;
      else w = 1 - x;
    }
    if (y + h > 1) {
      if (drag?.handle === "move") y = 1 - h;
      else h = 1 - y;
    }
    return { ...r, x, y, w, h };
  }

  function style(r: PanelRect): string {
    return (
      `left:${r.x * 100}%;top:${r.y * 100}%;` +
      `width:${r.w * 100}%;height:${r.h * 100}%;z-index:${10 + r.z};`
    );
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="host"
  class:editing
  bind:this={host}
  onpointermove={onPointerMove}
  onpointerup={endDrag}
  onpointercancel={endDrag}
>
  {#each panels as p, i (p.kind)}
    {#if !isHidden(p.kind)}
    <div class="slot" style={style(p.rect)}>
      <!-- Das eigentliche Panel: Glass-Flaeche mit seinem Inhalt, wie im
           Normalbetrieb. Im Editier-Modus fangen wir Klicks nur zum Verschieben
           ab (sinkt sonst in den Panel-Inhalt). -->
      <div class="glass panel-frame">
        <div class="panel-body">
          {@render panel(p)}
        </div>
      </div>

      {#if editing}
        <!-- Bounding-Box wie bei einer selektierten Shape: Rahmen ueber dem
             Panel, Flaeche verschiebt, Griffe skalieren. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="bbox"
          onpointerdown={(e) => startDrag(e, "move", i)}
          title="Panel verschieben"
        >
          <span class="h nw" onpointerdown={(e) => startDrag(e, "nw", i)}></span>
          <span class="h n" onpointerdown={(e) => startDrag(e, "n", i)}></span>
          <span class="h ne" onpointerdown={(e) => startDrag(e, "ne", i)}></span>
          <span class="h e" onpointerdown={(e) => startDrag(e, "e", i)}></span>
          <span class="h se" onpointerdown={(e) => startDrag(e, "se", i)}></span>
          <span class="h s" onpointerdown={(e) => startDrag(e, "s", i)}></span>
          <span class="h sw" onpointerdown={(e) => startDrag(e, "sw", i)}></span>
          <span class="h w" onpointerdown={(e) => startDrag(e, "w", i)}></span>
        </div>
      {/if}
    </div>
    {/if}
  {/each}
</div>

<style>
  .host {
    position: absolute;
    inset: 0;
    pointer-events: none; /* Klicks gehen an den Canvas durch … */
  }
  .slot {
    position: absolute;
    pointer-events: auto; /* … nur die Panele fangen sie ab. */
    padding: 4px;
  }
  .panel-frame {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .panel-body {
    flex: 1;
    min-height: 0;
    min-width: 0; /* erlaubt horizontales Schrumpfen (Panel schmal ziehbar) */
    overflow: auto;
    padding: 8px;
  }

  /* Bounding-Box im Editier-Modus. Liegt ueber dem Panel; die Flaeche selbst
     verschiebt (cursor: move), die Griffe skalieren. */
  .bbox {
    position: absolute;
    inset: 4px; /* deckt die Slot-Polsterung ab, sitzt auf der Panel-Kante */
    border: 1.5px solid var(--accent);
    border-radius: 14px;
    cursor: move;
    z-index: 5;
  }
  .h {
    position: absolute;
    width: 11px;
    height: 11px;
    background: var(--accent);
    border: 1.5px solid #fff;
    border-radius: 3px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  /* Ecken */
  .nw { left: -6px; top: -6px; cursor: nwse-resize; }
  .ne { right: -6px; top: -6px; cursor: nesw-resize; }
  .se { right: -6px; bottom: -6px; cursor: nwse-resize; }
  .sw { left: -6px; bottom: -6px; cursor: nesw-resize; }
  /* Kantenmitten */
  .n { left: 50%; top: -6px; transform: translateX(-50%); cursor: ns-resize; }
  .s { left: 50%; bottom: -6px; transform: translateX(-50%); cursor: ns-resize; }
  .e { right: -6px; top: 50%; transform: translateY(-50%); cursor: ew-resize; }
  .w { left: -6px; top: 50%; transform: translateY(-50%); cursor: ew-resize; }
</style>
