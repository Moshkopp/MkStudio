<script lang="ts">
  // Laser-Control-Panel (ADR 0007). Gerätespezifisch: das Panel rendert NUR die
  // Aktionen, die der aktive Treiber meldet — kein fixer G-Code/Send-Knopf mehr.
  // Der aktive Laser wird hier per Dropdown gewählt; Anlegen/Verwalten in Settings.
  import type { LaserRegistry, JobParamsDto } from "./core";

  type SavedPanelState = JobParamsDto & { jog_step: number; jog_speed: number };
  const defaults: SavedPanelState = {
    start_mode: "absolut", anchor: 4, selection_only: false, jog_step: 10, jog_speed: 100,
  };
  function loadPanelState(): SavedPanelState {
    if (typeof localStorage === "undefined") return defaults;
    try {
      return { ...defaults, ...JSON.parse(localStorage.getItem("luxifer_laser_panel") ?? "{}") };
    } catch {
      return defaults;
    }
  }
  const saved = loadPanelState();

  let {
    registry,
    actions,
    connected,
    onselect,
    onaction,
    onexport,
    onparamschange,
    onjog,
    onhome,
    onreadposition,
    onopensettings,
  }: {
    registry: LaserRegistry | null;
    /** Aktions-Schlüssel des aktiven Treibers (z. B. "send_job", "frame"). */
    actions: string[];
    /** Verbindungsstatus für die LED. */
    connected: boolean;
    onselect: (id: string) => void;
    onaction: (action: string, params: JobParamsDto) => void;
    onexport: (params: JobParamsDto) => void;
    onparamschange: (params: JobParamsDto) => void;
    onjog: (dx: number, dy: number, speed: number) => void;
    onhome: (speed: number) => void;
    onreadposition: () => void;
    onopensettings: () => void;
  } = $props();

  let startFrom = $state<"absolut" | "aktuell" | "ursprung">(saved.start_mode);
  // Job-Nullpunkt-Anker: 3×3-Raster (Index 0..8), 4 = Mitte.
  let anchor = $state(saved.anchor);
  let selectionOnly = $state(saved.selection_only);
  // Jog-Parameter.
  let jogStep = $state(saved.jog_step);
  let jogSpeed = $state(saved.jog_speed);

  const profiles = $derived(registry?.profiles ?? []);
  const activeId = $derived(registry?.active_id ?? "");
  const hasLaser = $derived(profiles.length > 0);

  // Label + Glyph je Aktions-Schlüssel (neutral, treiberunabhängig).
  const ACTION_META: Record<string, { label: string; glyph: string; primary?: boolean }> = {
    send_job: { label: "An Laser senden", glyph: "⭑", primary: true },
    stream_gcode: { label: "G-Code streamen", glyph: "⭑", primary: true },
    export_file: { label: "Exportieren", glyph: "▤" },
    frame: { label: "Rahmen", glyph: "⧉" },
    rubber_frame: { label: "Gummiband", glyph: "◇" },
    pause: { label: "Pause", glyph: "Ⅱ" },
    home: { label: "Home 0/0", glyph: "⌂" },
    go_origin: { label: "Ursprung", glyph: "◎" },
    stop: { label: "Stopp", glyph: "■" },
  };
  const meta = (a: string) => ACTION_META[a] ?? { label: a, glyph: "▸" };

  // Primäre Aktion (Senden/Streamen) getrennt als breiter Knopf hervorheben.
  const primaryActions = $derived(actions.filter((a) => meta(a).primary));
  // Kachel-Aktionen: alles außer primär, Export und Home (die haben eigene UI).
  const tileActions = $derived(
    actions.filter((a) => !meta(a).primary && a !== "export_file" && a !== "home"),
  );
  const canExport = $derived(actions.includes("export_file"));

  function params(): JobParamsDto {
    return { start_mode: startFrom, anchor, selection_only: selectionOnly };
  }
  $effect(() => { startFrom; anchor; selectionOnly; onparamschange(params()); });
  $effect(() => {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("luxifer_laser_panel", JSON.stringify({
        ...params(), jog_step: jogStep, jog_speed: jogSpeed,
      }));
    }
  });
  // Eine Aktion auslösen — Export läuft über den Datei-Download-Callback.
  function trigger(a: string) {
    if (a === "export_file") onexport(params());
    else onaction(a, params());
  }
</script>

<div class="laser">
  <!-- Laser-Auswahl (Dropdown) + Verbindungs-LED -->
  <section>
    <div class="laser-head">
      <span class="label">Laser</span>
      <span class="conn" title={connected ? "Verbunden" : "Getrennt"}>
        <span class="led" class:on={connected}></span>
        {connected ? "verbunden" : "getrennt"}
      </span>
    </div>
    {#if hasLaser}
      <select
        value={activeId}
        onchange={(e) => onselect((e.currentTarget as HTMLSelectElement).value)}
      >
        {#each profiles as p (p.id)}
          <option value={p.id}>{p.name} · {p.kind}</option>
        {/each}
      </select>
    {:else}
      <button class="wide ghost" onclick={onopensettings}>
        Kein Laser — in Einstellungen anlegen
      </button>
    {/if}
    <button class="settings-link" onclick={onopensettings} title="Laser verwalten">
      ⚙ Einstellungen
    </button>
  </section>

  <div class="sep"></div>

  <!-- Job-Aktionen (vom aktiven Treiber gemeldet) -->
  <section>
    <span class="label">Job</span>
    <label class="check"><input type="checkbox" bind:checked={selectionOnly} /> Nur Auswahl lasern</label>
    {#if !hasLaser}
      <p class="hint">Lege zuerst einen Laser an, um Jobs zu senden.</p>
    {:else if actions.length === 0}
      <p class="hint">Dieser Treiber meldet keine Aktionen.</p>
    {:else}
      {#each primaryActions as a (a)}
        <button class="wide send" onclick={() => trigger(a)}>
          {meta(a).glyph} {meta(a).label}
        </button>
      {/each}
      {#if tileActions.length}
        <div class="grid3">
          {#each tileActions as a (a)}
            <button class="tile" onclick={() => trigger(a)}>
              <span class="glyph">{meta(a).glyph}</span><span>{meta(a).label}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if canExport}
        <button class="wide" onclick={() => onexport(params())}>▤ Als Datei exportieren</button>
      {/if}
    {/if}
  </section>

  <div class="sep"></div>

  <!-- Job-Parameter (geräteneutral) -->
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

  {#if hasLaser}
    <div class="sep"></div>

    <!-- Kopf-Steuerung (Jog) -->
    <section>
      <span class="label">Kopf (Jog)</span>
      <div class="jog">
        <button style="grid-area: up" onclick={() => onjog(0, -jogStep, jogSpeed)} aria-label="hoch">↑</button>
        <button style="grid-area: left" onclick={() => onjog(-jogStep, 0, jogSpeed)} aria-label="links">←</button>
        <button style="grid-area: home" onclick={() => onhome(jogSpeed)} title="Referenzfahrt (0/0)">⌂</button>
        <button style="grid-area: right" onclick={() => onjog(jogStep, 0, jogSpeed)} aria-label="rechts">→</button>
        <button style="grid-area: down" onclick={() => onjog(0, jogStep, jogSpeed)} aria-label="runter">↓</button>
      </div>
      <div class="jogparams">
        <label>Schritt mm<input type="number" bind:value={jogStep} min="0.1" step="0.1" /></label>
        <label>Speed mm/s<input type="number" bind:value={jogSpeed} min="1" /></label>
      </div>
      <button class="wide" onclick={onreadposition}>⊹ Position lesen</button>
    </section>
  {/if}
</div>

<style>
  .laser {
    display: flex;
    flex-direction: column;
    gap: 10px;
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
  .label {
    font-size: 11px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .laser-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .conn {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
  }
  .led {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--muted);
    box-shadow: none;
    transition: background 0.2s, box-shadow 0.2s;
  }
  .led.on {
    background: #3fb27f;
    box-shadow: 0 0 8px #3fb27f88;
  }
  .jog {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    grid-template-rows: repeat(3, 38px);
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
  .jogparams input {
    width: 100%;
  }
  .sublabel {
    font-size: 11px;
    color: var(--muted);
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
    margin: 0;
  }
  .wide {
    width: 100%;
  }
  .settings-link {
    align-self: flex-start;
    font-size: 11px;
    background: transparent;
    border: none;
    color: var(--muted);
    padding: 2px 0;
    cursor: pointer;
  }
  .settings-link:hover {
    color: var(--text);
  }
  .ghost {
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
  .send {
    background: linear-gradient(180deg, #48c78e, #37a877);
    color: white;
    font-weight: 600;
    border-color: rgba(120, 240, 190, 0.5);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.3),
      0 0 16px -4px rgba(63, 178, 127, 0.6);
  }
  .send:hover {
    background: linear-gradient(180deg, #55d49b, #3fb587);
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
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .apt.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  select {
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    color: var(--text);
    padding: 6px 8px;
    font-size: 13px;
  }
  select:focus {
    outline: none;
    border-color: var(--accent);
  }
  button {
    background: linear-gradient(
      180deg,
      hsl(var(--btn-h) var(--btn-s) calc(var(--btn-l) + 14%) / 0.42),
      hsl(var(--btn-h) var(--btn-s) var(--btn-l) / 0.34)
    );
    color: var(--text);
    border: 1px solid hsl(var(--btn-h) var(--btn-s) 80% / 0.28);
    border-radius: 9px;
    padding: 7px 10px;
    cursor: pointer;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.14),
      0 1px 2px rgba(0, 0, 0, 0.25);
    transition:
      background 0.16s ease,
      border-color 0.16s ease,
      transform 0.08s ease;
  }
  button:hover {
    background: linear-gradient(
      180deg,
      hsl(var(--btn-h) var(--btn-s) calc(var(--btn-l) + 16%) / 0.6),
      hsl(var(--btn-h) var(--btn-s) var(--btn-l) / 0.52)
    );
    border-color: hsl(var(--btn-h) var(--btn-s) 82% / 0.45);
  }
  button:active {
    transform: translateY(1px);
  }
</style>
