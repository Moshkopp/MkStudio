<script lang="ts">
  // Formen-Galerie als eigenes Panel (statt eingeklemmt im Werkzeug-Panel).
  // Datengetrieben aus dem Core-Katalog: eine neue Form dort erscheint hier
  // automatisch. Klick auf eine Kachel waehlt die Form und aktiviert zugleich
  // das Polygon-Werkzeug. Spaeter auch fuer Muster-Fuellung wiederverwendbar.
  import Icon, { type IconName } from "./Icon.svelte";
  import type { ShapeInfo } from "./core";

  let {
    shapes,
    activeShape,
    onpickshape,
  }: {
    // Datengetriebener Formen-Katalog (aus dem Core).
    shapes: ShapeInfo[];
    // Aktuell gewaehlte Form-`id` (z. B. "hex"); markiert die Kachel.
    activeShape: string;
    // Form gewaehlt (waehlt zugleich das Polygon-Werkzeug).
    onpickshape: (id: string) => void;
  } = $props();
</script>

<div class="panel">
  <div class="grid">
    {#each shapes as sh}
      <button
        class="gbtn shape"
        class:active={activeShape === sh.id}
        title={sh.label}
        aria-label={sh.label}
        aria-pressed={activeShape === sh.id}
        onclick={() => onpickshape(sh.id)}
      >
        <Icon name={sh.icon as IconName} fill />
        <span class="label">{sh.label}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  /* Passt sich der Panelbreite an: die Kacheln fuellen ein flexibles Raster,
     Icon + Label skalieren mit. Container-Query fuer icon-relative Groessen. */
  .panel {
    width: 100%;
    container-type: inline-size;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(56px, 1fr));
    gap: 6px;
    width: 100%;
  }
  .shape {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 8px 4px;
    color: var(--text);
    font-size: clamp(16px, 14cqw, 26px);
  }
  .shape .label {
    font-size: 10px;
    color: var(--muted);
    line-height: 1;
  }
  .shape.active {
    background: linear-gradient(
      180deg,
      hsl(var(--accent-h) var(--accent-s) calc(var(--accent-l) + 8%)),
      var(--accent)
    );
    color: white;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.25);
  }
  .shape.active .label {
    color: rgba(255, 255, 255, 0.85);
  }
</style>
