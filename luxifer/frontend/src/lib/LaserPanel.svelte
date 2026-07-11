<script lang="ts">
  // Laser-Control-Panel (ADR 0007). Gerätespezifisch: das Panel rendert NUR die
  // Aktionen, die der aktive Treiber meldet — kein fixer G-Code/Send-Knopf mehr.
  // Der aktive Laser wird hier per Dropdown gewählt; Anlegen/Verwalten in Settings.
  import type { LaserRegistry, JobParamsDto } from "./core";

  // Einheitliches Icon-Set: alle Glyphen als Inline-SVG auf demselben 24er-Grid,
  // gleiche Strichstärke (1.6), damit das Panel eine Bildsprache spricht statt
  // zusammengewürfelter Unicode-Zeichen. Pfade siehe {#snippet icon} im Markup.
  type IconName =
    | "send" | "export" | "frame" | "rubber" | "pause"
    | "home" | "origin" | "stop" | "position" | "gear" | "dot";

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
    hasJob,
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
    /** Ob überhaupt Geometrie zum Lasern da ist (steuert den Bereit-Zustand). */
    hasJob: boolean;
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
  // Jog-Parameter. Grenzen fürs Slider-Ziehen; präzise Werte per Zahl-Antippen.
  let jogStep = $state(saved.jog_step);
  let jogSpeed = $state(saved.jog_speed);
  const STEP_MIN = 0.1, STEP_MAX = 100;
  const SPEED_MIN = 1, SPEED_MAX = 1000;
  // Welches Zahlen-Feld gerade als exaktes Eingabefeld offen ist (Zahl antippen
  // ODER Doppeltipp auf den Slider). null = beide zeigen nur Slider + Wert.
  let editing = $state<null | "step" | "speed">(null);
  // Wert am Slider clampen (Speed ganzzahlig, Schritt auf 0.1 gerundet).
  const clampStep = (v: number) =>
    Math.min(STEP_MAX, Math.max(STEP_MIN, Math.round(v * 10) / 10));
  const clampSpeed = (v: number) =>
    Math.min(SPEED_MAX, Math.max(SPEED_MIN, Math.round(v)));
  // Fokussiert das gerade eingeblendete Eingabefeld und markiert den Inhalt,
  // damit man am Touchscreen sofort tippen kann.
  function autofocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  const profiles = $derived(registry?.profiles ?? []);
  const activeId = $derived(registry?.active_id ?? "");
  const hasLaser = $derived(profiles.length > 0);
  // Bereit = verbunden und es gibt Geometrie. Erst dann leuchtet der Sende-Knopf
  // grün auf; sonst bleibt er neutral (Farbe = Zustandssignal, nicht Deko).
  const ready = $derived(connected && hasJob);

  // Label + Icon-Schlüssel je Aktions-Schlüssel (neutral, treiberunabhängig).
  // Das Icon wird als einheitliches Inline-SVG gerendert (siehe <Icon/> unten).
  // tone = semantische Ampel-Farbe fürs Laser-Bedienfeld: Start grün, Pause
  //   orange, Stop rot, Ursprung blau, Rest neutral. Am Gerät muss man diese
  //   Aktionen an der Farbe erkennen, nicht erst am Label lesen.
  type Tone = "go" | "warn" | "stop" | "nav" | "neutral";
  const ACTION_META: Record<
    string,
    { label: string; icon: IconName; tone?: Tone }
  > = {
    send_job: { label: "Start", icon: "send", tone: "go" },
    stream_gcode: { label: "Start", icon: "send", tone: "go" },
    pause: { label: "Pause", icon: "pause", tone: "warn" },
    stop: { label: "Stopp", icon: "stop", tone: "stop" },
    go_origin: { label: "Ursprung", icon: "origin", tone: "nav" },
    frame: { label: "Rahmen", icon: "frame" },
    rubber_frame: { label: "Gummiband", icon: "rubber" },
    export_file: { label: "Exportieren", icon: "export" },
    home: { label: "Home 0/0", icon: "home" },
  };
  const meta = (a: string) => ACTION_META[a] ?? { label: a, icon: "dot" as IconName };

  // Feste 2×3-Anordnung (Reihenfolge = Skizze). Der erste passende Treiber-Key
  // je Slot wird gerendert; nicht gemeldete Slots bleiben leer. Start deckt
  // send_job ODER stream_gcode ab (je nach Treiber nur einer davon aktiv).
  const GRID_SLOTS: string[][] = [
    ["send_job", "stream_gcode"], ["pause"], ["stop"],
    ["go_origin"], ["frame"], ["rubber_frame"],
  ];
  const gridActions = $derived(
    GRID_SLOTS.map((keys) => keys.find((k) => actions.includes(k)) ?? null),
  );
  const hasGridAction = $derived(gridActions.some((a) => a !== null));
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

<!--
  Einheitliches Icon-Snippet. Alle Aktions- und Steuer-Icons laufen hierdurch,
  damit Strichstärke, Grid (24×24) und Ausrichtung identisch sind.
-->
{#snippet icon(name: IconName)}
  <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {#if name === "send"}
      <path d="M4 12l16-7-7 16-2.5-6.5L4 12z" />
    {:else if name === "export"}
      <path d="M12 3v11" /><path d="M8 8l4-4 4 4" /><path d="M4 15v4h16v-4" />
    {:else if name === "frame"}
      <rect x="4" y="4" width="16" height="16" rx="1.5" stroke-dasharray="3 3" />
    {:else if name === "rubber"}
      <rect x="4" y="4" width="16" height="16" rx="1.5" /><path d="M4 4l16 16M20 4L4 20" opacity="0.4" />
    {:else if name === "pause"}
      <path d="M9 5v14M15 5v14" />
    {:else if name === "home"}
      <path d="M4 11l8-7 8 7" /><path d="M6 10v9h12v-9" />
    {:else if name === "origin"}
      <circle cx="12" cy="12" r="8" /><circle cx="12" cy="12" r="2" fill="currentColor" stroke="none" />
    {:else if name === "stop"}
      <rect x="6" y="6" width="12" height="12" rx="1.5" />
    {:else if name === "position"}
      <circle cx="12" cy="12" r="3" /><path d="M12 2v4M12 18v4M2 12h4M18 12h4" />
    {:else if name === "gear"}
      <circle cx="12" cy="12" r="3" /><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2 2M16.4 16.4l2 2M18.4 5.6l-2 2M7.6 16.4l-2 2" />
    {:else}
      <circle cx="12" cy="12" r="2.5" fill="currentColor" stroke="none" />
    {/if}
  </svg>
{/snippet}

<!--
  Touch-Zahlenfeld: Wert oben (antippbar → wird exaktes Eingabefeld), darunter
  ein großer Slider fürs grobe Ziehen. Doppeltipp auf den Slider öffnet ebenfalls
  die exakte Eingabe. So ist beides erreichbar — schnell mit dem Finger UND präzise.
-->
{#snippet numfield(
  key: "step" | "speed",
  label: string,
  unit: string,
  value: number,
  min: number,
  max: number,
  step: number,
  set: (v: number) => void,
)}
  <div class="numfield">
    <div class="numhead">
      <span class="numlabel">{label}</span>
      {#if editing === key}
        <input
          class="numinput"
          type="number"
          {min}
          {max}
          {step}
          value={value}
          oninput={(e) => set(Number((e.currentTarget as HTMLInputElement).value))}
          onblur={() => (editing = null)}
          onkeydown={(e) => { if (e.key === "Enter") editing = null; }}
          use:autofocus
        />
      {:else}
        <button class="numval" onclick={() => (editing = key)} title="Zum genauen Eingeben antippen">
          {value}<span class="numunit">{unit}</span>
        </button>
      {/if}
    </div>
    <input
      class="slider"
      type="range"
      {min}
      {max}
      {step}
      value={value}
      oninput={(e) => set(Number((e.currentTarget as HTMLInputElement).value))}
      ondblclick={() => (editing = key)}
      aria-label={label}
    />
  </div>
{/snippet}

<div class="laser">
  <!-- Laser-Auswahl (Dropdown) + Verbindungs-Status -->
  <section>
    <div class="row-between">
      <span class="label">Laser</span>
      <span class="status" class:on={connected}>
        <span class="led"></span>
        {connected ? "verbunden" : "getrennt"}
      </span>
    </div>
    {#if hasLaser}
      <div class="select-row">
        <select
          value={activeId}
          onchange={(e) => onselect((e.currentTarget as HTMLSelectElement).value)}
        >
          {#each profiles as p (p.id)}
            <option value={p.id}>{p.name} · {p.kind}</option>
          {/each}
        </select>
        <button class="icon-btn" onclick={onopensettings} title="Laser verwalten" aria-label="Laser verwalten">
          {@render icon("gear")}
        </button>
      </div>
    {:else}
      <button class="wide ghost" onclick={onopensettings}>
        Kein Laser — in Einstellungen anlegen
      </button>
    {/if}
  </section>

  <!-- Job-Aktionen (vom aktiven Treiber gemeldet), als Ampel-Grid 2×3. -->
  <section>
    <span class="label">Job</span>
    {#if !hasLaser}
      <p class="hint">Lege zuerst einen Laser an, um Jobs zu senden.</p>
    {:else if !hasGridAction}
      <p class="hint">Dieser Treiber meldet keine Aktionen.</p>
    {:else}
      <div class="grid">
        {#each gridActions as a, i (i)}
          {#if a}
            <button class="cell {meta(a).tone ?? 'neutral'}" onclick={() => trigger(a)}>
              {@render icon(meta(a).icon)}
              <span>{meta(a).label}</span>
            </button>
          {:else}
            <span class="cell empty"></span>
          {/if}
        {/each}
      </div>
      <label class="check">
        <input type="checkbox" bind:checked={selectionOnly} />
        <span>Nur Auswahl lasern</span>
      </label>
      {#if canExport}
        <div class="subsep"></div>
        <button class="wide subtle" onclick={() => onexport(params())}>
          {@render icon("export")}
          <span>Als Datei exportieren</span>
        </button>
      {/if}
    {/if}
  </section>

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
    <div class="field">
      Job-Nullpunkt
      <div class="anchor">
        {#each Array(9) as _, i}
          <button
            class="apt"
            class:on={anchor === i}
            onclick={() => (anchor = i)}
            aria-label={`Anker ${i}`}
          ><span class="apt-dot"></span></button>
        {/each}
      </div>
    </div>
  </section>

  {#if hasLaser}
    <!-- Kopf-Steuerung (Jog): großes Kreuz zentriert, Werte darunter (Touch). -->
    <section>
      <span class="label">Kopf</span>
      <div class="jog">
        <button style="grid-area: up" onclick={() => onjog(0, -jogStep, jogSpeed)} aria-label="hoch">
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5M6 11l6-6 6 6" /></svg>
        </button>
        <button style="grid-area: left" onclick={() => onjog(-jogStep, 0, jogSpeed)} aria-label="links">
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M19 12H5M11 6l-6 6 6 6" /></svg>
        </button>
        <button class="jog-home" style="grid-area: home" onclick={() => onhome(jogSpeed)} title="Referenzfahrt (0/0)" aria-label="Home">
          {@render icon("home")}
        </button>
        <button style="grid-area: right" onclick={() => onjog(jogStep, 0, jogSpeed)} aria-label="rechts">
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6" /></svg>
        </button>
        <button style="grid-area: down" onclick={() => onjog(0, jogStep, jogSpeed)} aria-label="runter">
          <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M6 13l6 6 6-6" /></svg>
        </button>
      </div>
      {@render numfield("step", "Schritt", "mm", jogStep, STEP_MIN, STEP_MAX, 0.1, (v) => (jogStep = clampStep(v)))}
      {@render numfield("speed", "Speed", "mm/s", jogSpeed, SPEED_MIN, SPEED_MAX, 1, (v) => (jogSpeed = clampSpeed(v)))}
      <button class="wide subtle" onclick={onreadposition}>
        {@render icon("position")}
        <span>Position lesen</span>
      </button>
    </section>
  {/if}
</div>

<style>
  /* ---------------------------------------------------------------------------
     Ein Stil-System für das ganze Panel: eine Button-Grundform mit Varianten,
     ein Icon-Grid, konsistente Abstände. Sektionen werden durch dünne
     Hairlines getrennt (::before), nicht durch separate Divs.
  --------------------------------------------------------------------------- */
  .laser {
    display: flex;
    flex-direction: column;
    --gap: 9px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    padding: 14px 0;
    position: relative;
  }
  section:first-child {
    padding-top: 2px;
  }
  section + section::before {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--border-soft);
  }

  .label {
    font-size: 10px;
    letter-spacing: 1.2px;
    text-transform: uppercase;
    color: var(--muted);
    font-weight: 600;
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
    margin: 0;
    line-height: 1.4;
  }

  .row-between {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  /* Verbindungs-Status */
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    transition: background 0.2s, box-shadow 0.2s;
  }
  .status.on {
    color: var(--text);
  }
  .status.on .led {
    background: #3fb27f;
    box-shadow: 0 0 8px #3fb27f88;
  }

  /* ---- Icons: ein Grid, eine Strichstärke ---- */
  .ic {
    width: 18px;
    height: 18px;
    flex: none;
  }

  /* ---- Buttons: eine Grundform ---- */
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text);
    background: linear-gradient(
      180deg,
      hsl(var(--btn-h) var(--btn-s) calc(var(--btn-l) + 14%) / 0.42),
      hsl(var(--btn-h) var(--btn-s) var(--btn-l) / 0.34)
    );
    border: 1px solid hsl(var(--btn-h) var(--btn-s) 80% / 0.28);
    border-radius: 9px;
    padding: 8px 10px;
    font-size: 13px;
    cursor: pointer;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.14),
      0 1px 2px rgba(0, 0, 0, 0.25);
    transition:
      background 0.16s ease,
      border-color 0.16s ease,
      color 0.16s ease,
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
  .wide {
    width: 100%;
  }
  .ghost {
    color: var(--muted);
    font-size: 12px;
  }

  /* Sekundäre Aktionen (Export, Position lesen): flacher, weniger Gewicht. */
  .subtle {
    background: hsl(var(--btn-h) var(--btn-s) var(--btn-l) / 0.16);
    border-color: hsl(var(--btn-h) var(--btn-s) 80% / 0.16);
    color: var(--muted);
    box-shadow: none;
    font-size: 12px;
  }
  .subtle:hover {
    background: hsl(var(--btn-h) var(--btn-s) var(--btn-l) / 0.3);
    color: var(--text);
  }

  /* Icon-only-Knopf (Einstellungen neben dem Dropdown). */
  .icon-btn {
    flex: none;
    padding: 8px;
    color: var(--muted);
  }
  .icon-btn:hover {
    color: var(--text);
  }
  .select-row {
    display: flex;
    gap: 6px;
  }
  .select-row select {
    flex: 1;
    min-width: 0;
  }

  /* Ampel-Grid 2×3: quadratische Aktions-Kacheln, Icon oben, Label unten.
     Reihenfolge (Skizze): Start | Pause | Stopp / Ursprung | Rahmen | Gummiband. */
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .cell {
    flex-direction: column;
    gap: 6px;
    aspect-ratio: 1;
    padding: 6px 4px;
    font-size: 11px;
    font-weight: 500;
  }
  .cell .ic {
    width: 22px;
    height: 22px;
  }
  .cell.empty {
    background: none;
    border: none;
    box-shadow: none;
    pointer-events: none;
  }

  /* Semantische Ampel-Töne. Basis-Verlauf + farbiger Rand/Glow, damit man die
     Funktion am Gerät sofort an der Farbe erkennt. */
  .cell.go {
    color: #eafff5;
    background: linear-gradient(180deg, hsl(150 52% 46%), hsl(150 50% 37%));
    border-color: hsl(150 58% 58% / 0.55);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.24), 0 0 16px -6px hsl(150 58% 50% / 0.7);
  }
  .cell.go:hover {
    background: linear-gradient(180deg, hsl(150 55% 51%), hsl(150 52% 41%));
    border-color: hsl(150 60% 62% / 0.7);
  }
  .cell.warn {
    color: #241800;
    background: linear-gradient(180deg, hsl(38 92% 60%), hsl(34 90% 50%));
    border-color: hsl(38 95% 68% / 0.6);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.3), 0 0 16px -6px hsl(38 92% 55% / 0.7);
  }
  .cell.warn:hover {
    background: linear-gradient(180deg, hsl(38 95% 64%), hsl(34 92% 54%));
    border-color: hsl(38 96% 72% / 0.8);
  }
  .cell.stop {
    color: #fff0ee;
    background: linear-gradient(180deg, hsl(2 72% 55%), hsl(2 68% 45%));
    border-color: hsl(2 78% 66% / 0.6);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.24), 0 0 16px -6px hsl(2 72% 52% / 0.7);
  }
  .cell.stop:hover {
    background: linear-gradient(180deg, hsl(2 74% 59%), hsl(2 70% 49%));
    border-color: hsl(2 80% 70% / 0.8);
  }
  .cell.nav {
    color: #eef4ff;
    background: linear-gradient(180deg, hsl(212 58% 52% / 0.55), hsl(212 55% 42% / 0.45));
    border-color: hsl(212 65% 62% / 0.5);
  }
  .cell.nav:hover {
    background: linear-gradient(180deg, hsl(212 60% 56% / 0.7), hsl(212 57% 46% / 0.6));
    border-color: hsl(212 66% 66% / 0.7);
  }

  /* Dünner Innen-Trenner (z. B. vor dem Export), leichter als die Sektionslinie. */
  .subsep {
    height: 1px;
    background: var(--border-soft);
    margin: 2px 0;
  }

  /* Touch-taugliche Checkbox: großes Ziel, klar sichtbarer Haken. */
  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    color: var(--text);
    cursor: pointer;
    padding: 4px 0;
  }
  .check input {
    appearance: none;
    -webkit-appearance: none;
    width: 22px;
    height: 22px;
    flex: none;
    margin: 0;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.28);
    border: 1px solid rgba(255, 255, 255, 0.18);
    cursor: pointer;
    position: relative;
    transition: background 0.14s, border-color 0.14s;
  }
  .check input:checked {
    background: var(--accent);
    border-color: var(--accent);
  }
  .check input:checked::after {
    content: "";
    position: absolute;
    left: 7px;
    top: 3px;
    width: 5px;
    height: 10px;
    border: solid #fff;
    border-width: 0 2.4px 2.4px 0;
    transform: rotate(45deg);
  }

  /* ---- Formularfelder ---- */
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
  }
  select {
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: var(--text);
    padding: 11px 10px;
    font-size: 14px;
  }
  select:focus {
    outline: none;
    border-color: var(--accent);
  }

  /* Job-Nullpunkt-Anker (3×3), touch-tauglich: große Felder, Ankerlage als
     kleiner Punkt in der jeweiligen Ecke/Kante. */
  .anchor {
    display: grid;
    grid-template-columns: repeat(3, 44px);
    gap: 5px;
    align-self: center;
  }
  .apt {
    padding: 0;
    width: 44px;
    height: 44px;
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow: none;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .apt-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--muted);
    transition: background 0.14s, transform 0.14s;
  }
  .apt:hover {
    border-color: hsl(var(--accent-h) var(--accent-s) 70% / 0.5);
    background: rgba(0, 0, 0, 0.25);
  }
  .apt.on,
  .apt.on:hover {
    background: hsl(var(--accent-h) var(--accent-s) var(--accent-l) / 0.18);
    border-color: var(--accent);
  }
  .apt.on .apt-dot {
    background: var(--accent);
    transform: scale(1.4);
  }

  /* ---- Jog: großes zentriertes Kreuz ---- */
  .jog {
    align-self: center;
    display: grid;
    grid-template-columns: repeat(3, 56px);
    grid-template-rows: repeat(3, 56px);
    gap: 6px;
    grid-template-areas:
      ". up ."
      "left home right"
      ". down .";
  }
  .jog button {
    padding: 0;
  }
  .jog .ic {
    width: 24px;
    height: 24px;
  }
  .jog-home {
    color: var(--muted);
  }
  .jog-home:hover {
    color: var(--text);
  }

  /* ---- Touch-Zahlenfeld: Wert oben (antippbar), Slider darunter ---- */
  .numfield {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .numhead {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    min-height: 30px;
  }
  .numlabel {
    font-size: 13px;
    color: var(--muted);
  }
  .numval {
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 2px 6px;
    font-size: 18px;
    font-weight: 600;
    color: var(--text);
    border-radius: 6px;
    line-height: 1;
    gap: 3px;
  }
  .numval:hover {
    background: rgba(255, 255, 255, 0.06);
    border: none;
  }
  .numunit {
    font-size: 11px;
    font-weight: 400;
    color: var(--muted);
  }
  .numinput {
    width: 96px;
    text-align: right;
    background: rgba(0, 0, 0, 0.28);
    border: 1px solid var(--accent);
    border-radius: 8px;
    color: var(--text);
    padding: 6px 8px;
    font-size: 16px;
    font-weight: 600;
  }
  .numinput:focus {
    outline: none;
  }

  /* Großer Slider mit dickem Track und fettem Daumen (Finger-Ziel). */
  .slider {
    appearance: none;
    -webkit-appearance: none;
    width: 100%;
    height: 26px;
    background: transparent;
    cursor: pointer;
    margin: 0;
  }
  .slider::-webkit-slider-runnable-track {
    height: 8px;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .slider::-moz-range-track {
    height: 8px;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 26px;
    height: 26px;
    margin-top: -10px;
    border-radius: 50%;
    background: linear-gradient(180deg, hsl(var(--accent-h) var(--accent-s) calc(var(--accent-l) + 8%)), var(--accent));
    border: 1px solid hsl(var(--accent-h) var(--accent-s) 82% / 0.7);
    box-shadow: 0 2px 6px -1px rgba(0, 0, 0, 0.5);
  }
  .slider::-moz-range-thumb {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: linear-gradient(180deg, hsl(var(--accent-h) var(--accent-s) calc(var(--accent-l) + 8%)), var(--accent));
    border: 1px solid hsl(var(--accent-h) var(--accent-s) 82% / 0.7);
    box-shadow: 0 2px 6px -1px rgba(0, 0, 0, 0.5);
  }
  .slider:focus {
    outline: none;
  }
</style>
