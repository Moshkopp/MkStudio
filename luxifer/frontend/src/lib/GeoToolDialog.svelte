<script lang="ts">
  // Optionen-Dialog der Geometrie-Werkzeuge aus der Toolbar (Referenz-UX v1:
  // Werkzeug-Button → Parameter → auf die AUSWAHL anwenden).
  // kind = "boolean" | "fillet" | "offset" | "pattern".
  import type { BoolOpKind, PatternKind } from "./core";

  let {
    kind,
    selCount,
    onboolean,
    onfillet,
    onfilletpick,
    onoffset,
    onpattern,
    onclose,
  }: {
    kind: "boolean" | "fillet" | "offset" | "pattern";
    selCount: number;
    onboolean: (op: BoolOpKind) => void;
    onfillet: (radius: number) => void;
    onoffset: (dist: number) => void;
    onpattern: (p: PatternKind, gapX: number, gapY: number, angle: number, size: number) => void;
    // Ecken-Pick-Modus starten (Fillet: Ecken einzeln anklicken).
    onfilletpick: (radius: number) => void;
    onclose: () => void;
  } = $props();

  let radius = $state(2.0);
  let dist = $state(2.0);
  // Pattern-Parameter (Defaults wie die Referenz: gap 4mm, size 2mm).
  let pattern = $state<PatternKind>("lines");
  let gapX = $state(4.0);
  let gapY = $state(4.0);
  let angle = $state(0);
  let size = $state(2.0);

  const titles = {
    boolean: "Boolean (Auswahl)",
    fillet: "Ecken verrunden",
    offset: "Offset / parallele Kontur",
    pattern: "Muster füllen",
  } as const;
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="button" tabindex="-1">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>{titles[kind]}</h3>

    {#if selCount === 0}
      <p class="hint">Zuerst Formen auswählen, dann das Werkzeug anwenden.</p>
    {/if}

    {#if kind === "boolean"}
      <p class="hint">Wirkt auf ≥ 2 geschlossene Formen. Subjekt = zuerst gewählte Form.</p>
      <div class="row3">
        <button class="op" disabled={selCount < 2} onclick={() => onboolean("union")}>∪<span>Vereinigen</span></button>
        <button class="op" disabled={selCount < 2} onclick={() => onboolean("intersect")}>∩<span>Schneiden</span></button>
        <button class="op" disabled={selCount < 2} onclick={() => onboolean("diff")}>∖<span>Abziehen</span></button>
      </div>
    {:else if kind === "fillet"}
      <label>Radius (mm)<input type="number" step="0.5" min="0.1" bind:value={radius} /></label>
      <button class="primary" disabled={selCount < 1} onclick={() => onfillet(radius)}>Alle Ecken der Auswahl</button>
      <button class="primary" onclick={() => onfilletpick(radius)}>Ecken einzeln anklicken…</button>
    {:else if kind === "offset"}
      <label>Abstand (mm, negativ = innen)<input type="number" step="0.5" bind:value={dist} /></label>
      <button class="primary" disabled={selCount < 1} onclick={() => onoffset(dist)}>Parallele Kontur erzeugen</button>
    {:else}
      <label>
        Muster
        <select bind:value={pattern}>
          <option value="lines">Linien</option>
          <option value="circles">Kreise</option>
          <option value="slots">Slots (Langlöcher)</option>
          <option value="hex">Waben</option>
        </select>
      </label>
      <div class="row">
        <label>Abstand X (mm)<input type="number" step="0.5" min="0.1" bind:value={gapX} /></label>
        <label>Abstand Y (mm)<input type="number" step="0.5" min="0.1" bind:value={gapY} /></label>
      </div>
      <div class="row">
        <label>Winkel (°)<input type="number" step="5" bind:value={angle} /></label>
        {#if pattern !== "lines"}
          <label>Größe (mm)<input type="number" step="0.5" min="0.2" bind:value={size} /></label>
        {/if}
      </div>
      <p class="hint">Wirkt auf die selektierten geschlossenen Formen; eine innere Kontur wird als Loch ausgespart.</p>
      <button class="primary" disabled={selCount < 1} onclick={() => onpattern(pattern, gapX, gapY, angle, size)}>
        Füllen
      </button>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: #1c1e24;
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 18px;
    width: min(340px, 92vw);
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 18px 50px -12px rgba(0, 0, 0, 0.7);
  }
  h3 {
    margin: 0;
    font-size: 15px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12px;
    color: var(--muted);
    flex: 1;
  }
  input,
  select {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 7px 8px;
    font-size: 13px;
  }
  .row {
    display: flex;
    gap: 10px;
  }
  .row3 {
    display: flex;
    gap: 8px;
  }
  .op {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 10px 4px;
    font-size: 20px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--border);
    border-radius: 8px;
    color: var(--text);
    cursor: pointer;
  }
  .op span {
    font-size: 11px;
    color: var(--muted);
  }
  .op:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    border: 1px solid var(--accent);
    border-radius: 7px;
    padding: 9px;
    font-size: 13px;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
