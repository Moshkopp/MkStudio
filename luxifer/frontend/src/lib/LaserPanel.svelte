<script lang="ts">
  // Laser-Control-Panel (UI-Gerüst nach ThorBurn-Vorbild).
  // Verbindung/Jog sind noch Platzhalter; "Start" erzeugt bereits G-Code.
  let { ongenerate }: { ongenerate: () => void } = $props();

  let connected = $state(false);
  let startFrom = $state<"absolut" | "aktuell" | "ursprung">("absolut");
  // Job-Nullpunkt-Anker: 3×3-Raster (Index 0..8), 4 = Mitte.
  let anchor = $state(4);
  let jogStep = $state(10);
  let jogSpeed = $state(100);

  // Platzhalter-Position (bis echte Verbindung existiert).
  const posX = $state(0);
  const posY = $state(0);

  function todo() {
    /* Platzhalter — Aktion folgt mit Treiber/Transport. */
  }
</script>

<div class="panel laser">
  <!-- Verbindung -->
  <section>
    <div class="head">
      <span class="dot" class:on={connected}></span>
      <span class="title">{connected ? "Online" : "Getrennt"}</span>
    </div>
    <button class="wide" onclick={() => (connected = !connected)}>
      {connected ? "Verbindung trennen" : "Verbindung aufbauen"}
    </button>
    <div class="pos">
      <span>X {posX.toFixed(2)} mm</span>
      <span>Y {posY.toFixed(2)} mm</span>
    </div>
  </section>

  <div class="sep"></div>

  <!-- Job-Aktionen -->
  <section>
    <span class="label">Job</span>
    <div class="grid3">
      <button class="tile start" onclick={ongenerate} title="Job erzeugen (G-Code)">
        <span class="glyph">▶</span><span>Start</span>
      </button>
      <button class="tile" onclick={todo}><span class="glyph">⏸</span><span>Pause</span></button>
      <button class="tile" onclick={todo}><span class="glyph">■</span><span>Stopp</span></button>
      <button class="tile" onclick={todo}><span class="glyph">⌂</span><span>Ursprung</span></button>
      <button class="tile" onclick={todo}><span class="glyph">⧉</span><span>Rahmen</span></button>
      <button class="tile" onclick={todo}><span class="glyph">◇</span><span>Kontur</span></button>
    </div>
  </section>

  <div class="sep"></div>

  <!-- Job-Parameter -->
  <section>
    <span class="label">Parameter</span>
    <label class="field">
      Starten von
      <select bind:value={startFrom}>
        <option value="absolut">Absolute Koordinaten</option>
        <option value="aktuell">Aktuelle Position</option>
        <option value="ursprung">Benutzerursprung</option>
      </select>
    </label>
    <div class="anchor-wrap">
      <span class="sublabel">Job-Nullpunkt</span>
      <div class="anchor">
        {#each Array(9) as _, i}
          <button
            class="apt"
            class:on={anchor === i}
            onclick={() => (anchor = i)}
            aria-label={`Anker ${i}`}
          ></button>
        {/each}
      </div>
    </div>
  </section>

  <div class="sep"></div>

  <!-- Kopf-Steuerung (Jog) -->
  <section>
    <span class="label">Kopf (Jog)</span>
    <div class="jog">
      <button style="grid-area: up" onclick={todo}>↑</button>
      <button style="grid-area: left" onclick={todo}>←</button>
      <button style="grid-area: home" onclick={todo} title="Home">⌂</button>
      <button style="grid-area: right" onclick={todo}>→</button>
      <button style="grid-area: down" onclick={todo}>↓</button>
    </div>
    <div class="jogparams">
      <label>Schritt mm<input type="number" bind:value={jogStep} min="0.1" step="0.1" /></label>
      <label>Speed mm/s<input type="number" bind:value={jogSpeed} min="1" /></label>
    </div>
  </section>
</div>

<style>
  .laser {
    position: absolute;
    right: 12px;
    bottom: 12px;
    width: 240px;
    max-height: calc(100% - 24px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 40px -4px rgba(0, 0, 0, 0.5);
    padding: 12px;
    z-index: 10;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sep {
    height: 1px;
    background: var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 5px;
    background: var(--muted);
  }
  .dot.on {
    background: #3fb27f;
    box-shadow: 0 0 8px #3fb27f88;
  }
  .title {
    font-weight: 500;
  }
  .label {
    font-size: 11px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sublabel {
    font-size: 11px;
    color: var(--muted);
  }
  .wide {
    width: 100%;
  }
  .pos {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--muted);
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 8px 4px;
    font-size: 11px;
  }
  .tile .glyph {
    font-size: 16px;
  }
  .tile.start {
    background: var(--accent);
    color: white;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .anchor-wrap {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .anchor {
    display: grid;
    grid-template-columns: repeat(3, 14px);
    grid-template-rows: repeat(3, 14px);
    gap: 3px;
  }
  .apt {
    padding: 0;
    width: 14px;
    height: 14px;
    border-radius: 3px;
    background: #16171b;
    border: 1px solid var(--border);
  }
  .apt.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .jog {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    grid-template-rows: repeat(3, 40px);
    gap: 4px;
    grid-template-areas:
      ". up ."
      "left home right"
      ". down .";
  }
  .jog button {
    font-size: 16px;
  }
  .jogparams {
    display: flex;
    gap: 8px;
  }
  .jogparams label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
  }
  input,
  select {
    background: #16171b;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 6px 8px;
    font-size: 13px;
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: var(--accent);
  }
  button {
    background: #26282d;
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 7px 10px;
    cursor: pointer;
    transition: background 0.14s;
  }
  button:hover {
    background: #2e3036;
  }
</style>
