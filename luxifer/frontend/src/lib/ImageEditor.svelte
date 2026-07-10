<script lang="ts">
  // Bild-Editor (ADR 0004 §3): Doppelklick auf ein Bild öffnet diesen Dialog.
  // Live-Vorschau + Regler. Alle Werte sind nicht-destruktive Parameter; das
  // Original im Store bleibt unangetastet. Beim Schließen werden die Parameter
  // an den Core geschrieben (set_image_params).
  import { imageRender, type ImageParams } from "./core";

  let {
    asset,
    params,
    onapply,
    onclose,
    onvectorize,
  }: {
    // Store-Asset-ID des Bild-Shapes.
    asset: string;
    // Startparameter (aus dem Shape).
    params: ImageParams;
    // Übernimmt die Parameter in den Core (Live, bei jeder Änderung).
    onapply: (p: ImageParams) => void;
    onclose: () => void;
    // Vektorisiert das Bild (Trace) mit Schwelle + Invert; schließt den Dialog.
    onvectorize: (threshold: number, invert: boolean) => void;
  } = $props();

  // Lokale, editierbare Kopie. Startwert beim Öffnen.
  /* svelte-ignore state_referenced_locally */
  let p = $state<ImageParams>({ ...params });

  // Vorschaubild (Data-URL). Wird bei jeder Parameteränderung neu gerendert.
  let previewUrl = $state<string | null>(null);
  // Laufende Render-Anfrage entprellen: nur die letzte zählt.
  let renderToken = 0;

  // Ob die Laser-Vorschau (invert_laser) statt der Editor-Ansicht gezeigt wird.
  let showLaser = $state(false);

  // Render-Effect: hängt NUR an den Parametern + der Ansicht. Bewusst getrennt
  // von onapply — sonst löst das Parent-Update (neue Scene) einen Re-Run aus,
  // der die laufende Render-Anfrage per Token verwirft und die Vorschau auf
  // „Rendere…" hängen lässt.
  $effect(() => {
    const snap: ImageParams = {
      mode: p.mode,
      threshold: p.threshold,
      brightness: p.brightness,
      contrast: p.contrast,
      gamma: p.gamma,
      invert_editor: p.invert_editor,
      invert_laser: p.invert_laser,
    };
    const laser = showLaser;
    const token = ++renderToken;
    const invert = laser ? snap.invert_laser : snap.invert_editor;
    imageRender(asset, snap, invert).then((url) => {
      if (token === renderToken) previewUrl = url;
    });
  });

  // onapply getrennt: schreibt die Parameter live in den Core (Canvas zieht mit).
  $effect(() => {
    onapply({
      mode: p.mode,
      threshold: p.threshold,
      brightness: p.brightness,
      contrast: p.contrast,
      gamma: p.gamma,
      invert_editor: p.invert_editor,
      invert_laser: p.invert_laser,
    });
  });

  function reset() {
    p = {
      mode: "Grayscale",
      threshold: 128,
      brightness: 0,
      contrast: 0,
      gamma: 1.0,
      invert_editor: false,
      invert_laser: false,
    };
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="backdrop"
  onclick={onclose}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  role="button"
  tabindex="-1"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="head">
      <h3>Bild bearbeiten</h3>
      <div class="viewtabs">
        <button class:active={!showLaser} onclick={() => (showLaser = false)}>Editor</button>
        <button class:active={showLaser} onclick={() => (showLaser = true)}>Laser-Vorschau</button>
      </div>
    </div>

    <div class="body">
      <!-- Live-Vorschau -->
      <div class="preview">
        {#if previewUrl}
          <img src={previewUrl} alt="Bildvorschau" />
        {:else}
          <span class="loading">Rendere…</span>
        {/if}
      </div>

      <!-- Regler -->
      <div class="controls">
        <label>
          Modus
          <select bind:value={p.mode}>
            <option value="Grayscale">Graustufe</option>
            <option value="Threshold">Schwellwert</option>
            <option value="Floyd">Dither: Floyd–Steinberg</option>
            <option value="Jarvis">Dither: Jarvis</option>
            <option value="Stucki">Dither: Stucki</option>
            <option value="Atkinson">Dither: Atkinson</option>
            <option value="Bayer">Dither: Bayer (Raster)</option>
            <option value="LaserRuns">Dither: Laser-Strecken</option>
          </select>
        </label>

        {#if p.mode === "Threshold"}
          <label>
            Schwelle <span class="val">{p.threshold}</span>
            <input type="range" min="0" max="255" bind:value={p.threshold} />
          </label>
        {/if}

        <label>
          Helligkeit <span class="val">{p.brightness}</span>
          <input type="range" min="-100" max="100" bind:value={p.brightness} />
        </label>
        <label>
          Kontrast <span class="val">{p.contrast}</span>
          <input type="range" min="-100" max="100" bind:value={p.contrast} />
        </label>
        <label>
          Gamma <span class="val">{p.gamma.toFixed(2)}</span>
          <input type="range" min="0.1" max="3" step="0.05" bind:value={p.gamma} />
        </label>

        <label class="check">
          <input type="checkbox" bind:checked={p.invert_editor} /> Invertieren (Editor)
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={p.invert_laser} /> Invertieren (Laser)
        </label>

        <div class="actions">
          <button class="ghost" onclick={reset}>Zurücksetzen</button>
          <button class="primary" onclick={onclose}>Fertig</button>
        </div>
        <button
          class="ghost trace"
          onclick={() => onvectorize(p.threshold, p.invert_editor)}
          title="Konturen des Motivs als Vektor-Pfade erzeugen (nutzt die Schwelle oben)"
        >
          ⌁ Vektorisieren (Trace)
        </button>
      </div>
    </div>
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
    width: min(760px, 92vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: 0 18px 50px -12px rgba(0, 0, 0, 0.7);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .viewtabs {
    display: flex;
    gap: 4px;
  }
  .viewtabs button {
    background: rgba(255, 255, 255, 0.06);
    color: var(--muted);
    border: 1px solid transparent;
    border-radius: 7px;
    padding: 5px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .viewtabs button.active {
    background: hsl(var(--accent-h) var(--accent-s) var(--accent-l) / 0.18);
    color: var(--text);
    border-color: hsl(var(--accent-h) var(--accent-s) 70% / 0.4);
  }
  .body {
    display: grid;
    grid-template-columns: 1fr 260px;
    gap: 16px;
    min-height: 0;
  }
  .preview {
    background: #0d0e11;
    border: 1px solid var(--border);
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 300px;
    overflow: hidden;
  }
  .preview img {
    max-width: 100%;
    max-height: 60vh;
    object-fit: contain;
    /* Schachbrett-freie, scharfe Darstellung auch bei starker Vergrößerung. */
    image-rendering: auto;
  }
  .loading {
    color: var(--muted);
    font-size: 13px;
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12px;
    color: var(--muted);
  }
  .val {
    color: var(--text);
    font-variant-numeric: tabular-nums;
    float: right;
  }
  select,
  input[type="range"] {
    width: 100%;
  }
  select {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 6px 8px;
    font-size: 13px;
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--text);
    font-size: 13px;
  }
  .check input {
    width: auto;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: auto;
    padding-top: 8px;
  }
  .actions button {
    flex: 1;
    border-radius: 7px;
    padding: 8px;
    font-size: 13px;
    cursor: pointer;
    border: 1px solid var(--border);
  }
  .ghost {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text);
  }
  .primary {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
  .trace {
    border-radius: 7px;
    padding: 8px;
    font-size: 13px;
    cursor: pointer;
    border: 1px dashed var(--border);
  }
</style>
