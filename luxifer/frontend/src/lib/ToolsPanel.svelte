<script lang="ts">
  // Werkzeugleiste nach ThorBurn-Vorbild: 21 Werkzeuge in 5 Gruppen. Aktuell
  // funktionieren select/rect/ellipse; die uebrigen sind als Stub eingebaut
  // (Icon vorhanden, "(Vorschau)"), damit die Leiste vollstaendig steht und
  // spaeter nur noch verdrahtet werden muss. Undo/Redo/Loeschen sind NICHT mehr
  // hier — die liegen im Header bzw. auf der Entf-Taste.
  import Icon, { type IconName } from "./Icon.svelte";

  type Tool = "select" | "rect" | "ellipse" | "line" | "polyline" | "polygon" | "spline" | "measure";
  // Sofort-Befehle auf der Auswahl (kein Zeichenmodus), z. B. Spiegeln.
  type Action = "mirror_h" | "mirror_v" | "text" | "boolean" | "fillet" | "offset" | "pattern-fill" | "coaster_rect" | "coaster_circle" | "bridge";
  let {
    tool,
    onpick,
    onaction,
  }: {
    tool: Tool;
    // Nur die real funktionierenden Werkzeuge werden nach oben gemeldet.
    onpick: (t: Tool) => void;
    // Sofort-Befehle (Spiegeln etc.) werden getrennt gemeldet.
    onaction: (a: Action) => void;
  } = $props();

  // Ein Werkzeug: Name, Icon, Tooltip. `active` = funktioniert als Werkzeug
  // (Zeichenmodus). `action` = Sofort-Befehl auf der Auswahl (funktioniert auch).
  type ToolDef = {
    name: string;
    icon: IconName;
    tip: string;
    active?: boolean;
    action?: boolean;
    wide?: boolean;
  };

  // Gruppen exakt wie in ThorBurn (docs/referenz + ToolBar.qml).
  const groups: ToolDef[][] = [
    // 1: Auswahl
    [{ name: "select", icon: "select", tip: "Auswahl / Verschieben", active: true, wide: true }],
    // 2: Zeichnen & Formen
    [
      { name: "rect", icon: "rect", tip: "Rechteck", active: true },
      { name: "ellipse", icon: "ellipse", tip: "Ellipse", active: true },
      { name: "polygon", icon: "polygon", tip: "Polygon (Form unten wählen, dann aufziehen)", active: true },
      { name: "line", icon: "line", tip: "Linie", active: true },
      { name: "polyline", icon: "polyline", tip: "Polylinie (Klicks setzen Punkte, Doppelklick/Enter schließt ab)", active: true },
      { name: "spline", icon: "spline", tip: "Spline (Klicks setzen Punkte, glatte Kurve hindurch)", active: true },
      { name: "bezier", icon: "bezier", tip: "Bézier-Feder" },
      { name: "text", icon: "text", tip: "Text einfügen (Text→Pfad)", action: true },
      { name: "node", icon: "node", tip: "Knoten bearbeiten" },
    ],
    // 3: Operationen & Hilfsmittel
    [
      { name: "trim", icon: "trim", tip: "Trimmen" },
      { name: "bridge", icon: "bridge", tip: "Haltesteg: Lücke in Kontur schneiden (Teil bleibt hängen)", action: true },
      { name: "boolean", icon: "boolean", tip: "Boolean: Vereinigen/Schneiden/Abziehen (Auswahl)", action: true },
      { name: "fillet", icon: "fillet", tip: "Ecken verrunden (Auswahl)", action: true },
      { name: "pattern-fill", icon: "pattern-fill", tip: "Muster füllen: Linien/Kreise/Slots/Waben (Auswahl)", action: true },
      { name: "offset", icon: "offset", tip: "Offset / parallele Kontur (Auswahl)", action: true },
      { name: "measure", icon: "measure", tip: "Messen (Klicken+Ziehen: Abstand in mm)", active: true },
    ],
    // 4: Spiegeln (Sofort-Befehle auf der Auswahl)
    [
      { name: "mirror_h", icon: "mirror-h", tip: "Horizontal spiegeln", action: true },
      { name: "mirror_v", icon: "mirror-v", tip: "Vertikal spiegeln", action: true },
    ],
    // 5: Untersetzer-Schnelleinfügung
    [
      { name: "coaster_rect", icon: "coaster-rect", tip: "4×2 eckige Untersetzer (100 mm) einfügen", action: true },
      { name: "coaster_circle", icon: "coaster-circle", tip: "4×2 runde Untersetzer (100 mm) einfügen", action: true },
    ],
  ];

  function click(t: ToolDef) {
    if (t.active) onpick(t.name as Tool);
    else if (t.action) onaction(t.name as Action);
    // Stubs tun (noch) nichts.
  }
</script>

<div class="tools">
  {#each groups as group, gi}
    <div class="group">
      {#each group as t}
        <button
          class="gbtn tool"
          class:wide={t.wide}
          class:active={t.active && tool === t.name}
          class:stub={!t.active && !t.action}
          title={t.tip + (t.active || t.action ? "" : " (Vorschau)")}
          aria-label={t.tip}
          onclick={() => click(t)}
        >
          <Icon name={t.icon} fill />
        </button>
      {/each}
    </div>
    {#if gi < groups.length - 1}
      <div class="divider"></div>
    {/if}
  {/each}
</div>

<style>
  /* Die Werkzeugleiste passt sich der Panelbreite an: Buttons fuellen die zwei
     Spalten und werden mit dem Panel groesser/kleiner (quadratisch), Icons
     skalieren mit. Kein fixes 34px mehr -> kein Stauchen bei schmalem Panel. */
  .tools {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 100%;
    container-type: inline-size;
  }
  .group {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    width: 100%;
    justify-items: stretch;
  }
  .tool {
    display: flex;
    align-items: center;
    justify-content: center;
    aspect-ratio: 1;
    width: 100%;
    /* In breiten Panels nicht ins Riesige wachsen. */
    max-width: 40px;
    justify-self: center;
    color: var(--text);
    /* Icon-Groesse folgt der Panelbreite. */
    font-size: clamp(12px, 7cqw, 18px);
  }
  /* Breiter Auswahl-Button ueber beide Spalten. */
  .tool.wide {
    grid-column: 1 / -1;
    width: 100%;
    max-width: none;
    aspect-ratio: auto;
    height: clamp(28px, 9cqw, 40px);
  }
  /* Stubs dezenter, damit klar ist, was schon geht. */
  .tool.stub {
    color: var(--muted);
    opacity: 0.7;
  }
  .tool.stub:hover {
    opacity: 1;
  }
  .divider {
    width: 80%;
    height: 1px;
    background: var(--border);
    margin: 1px 0;
  }
</style>
