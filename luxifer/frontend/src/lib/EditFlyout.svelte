<script lang="ts">
  // Flyout im Editier-Modus (ADR 0002 §3/§5): Akzent- und Button-Farbe mit
  // Intensitaets-Regler (Live-Vorschau), Rastermaße zum Experimentieren,
  // Arbeitsplatzname, sichtbare Panele je Reiter, Reset auf Standard.
  import type { UiSettings, PanelKind, Tab } from "./core";

  let {
    settings,
    tab,
    visiblePanels,
    onchange,
    ontogglepanel,
    onreset,
    onclose,
  }: {
    settings: UiSettings;
    tab: Tab;
    // Welche Panel-Arten sind im aktuellen Reiter sichtbar.
    visiblePanels: PanelKind[];
    // Aendert die Settings (Theming/Grid/Arbeitsplatz) — App speichert.
    onchange: (s: UiSettings) => void;
    ontogglepanel: (kind: PanelKind) => void;
    onreset: () => void;
    onclose: () => void;
  } = $props();

  // Alle Panel-Arten, die pro Reiter angeboten werden.
  const allPanels: { kind: PanelKind; label: string; tabs: Tab[] }[] = [
    { kind: "Werkzeuge", label: "Werkzeuge", tabs: ["Design"] },
    { kind: "Ebenen", label: "Ebenen", tabs: ["Design", "Laser"] },
    { kind: "Farbpalette", label: "Farbpalette", tabs: ["Design"] },
    { kind: "Formen", label: "Formen", tabs: ["Design"] },
    { kind: "Anordnen", label: "Anordnen", tabs: ["Design"] },
    { kind: "Laser", label: "Laser-Control", tabs: ["Laser"] },
    { kind: "JobStatus", label: "Job-Status", tabs: ["Monitor"] },
  ];
  const panelsForTab = $derived(allPanels.filter((p) => p.tabs.includes(tab)));

  function hex(rgb: [number, number, number]): string {
    return "#" + rgb.map((v) => v.toString(16).padStart(2, "0")).join("");
  }
  function fromHex(h: string): [number, number, number] {
    return [
      parseInt(h.slice(1, 3), 16),
      parseInt(h.slice(3, 5), 16),
      parseInt(h.slice(5, 7), 16),
    ];
  }

  function setAccentHue(h: string) {
    onchange({ ...settings, theme: { ...settings.theme, accent: { ...settings.theme.accent, hue: fromHex(h) } } });
  }
  function setAccentIntensity(v: number) {
    onchange({ ...settings, theme: { ...settings.theme, accent: { ...settings.theme.accent, intensity: v } } });
  }
  function setButtonHue(h: string) {
    onchange({ ...settings, theme: { ...settings.theme, button: { ...settings.theme.button, hue: fromHex(h) } } });
  }
  function setButtonIntensity(v: number) {
    onchange({ ...settings, theme: { ...settings.theme, button: { ...settings.theme.button, intensity: v } } });
  }
  function setWorkplace(v: string) {
    onchange({ ...settings, workplace: v });
  }
</script>

<div class="flyout glass">
  <div class="head">
    <span>Oberfläche bearbeiten</span>
    <button class="x" onclick={onclose} title="Editier-Modus verlassen">✕</button>
  </div>

  <section>
    <span class="cap">Akzentfarbe</span>
    <div class="row">
      <input type="color" value={hex(settings.theme.accent.hue)} oninput={(e) => setAccentHue(e.currentTarget.value)} />
      <input
        type="range" min="0.3" max="0.9" step="0.05"
        value={settings.theme.accent.intensity}
        oninput={(e) => setAccentIntensity(+e.currentTarget.value)}
      />
    </div>
    <span class="cap">Button-Farbe</span>
    <div class="row">
      <input type="color" value={hex(settings.theme.button.hue)} oninput={(e) => setButtonHue(e.currentTarget.value)} />
      <input
        type="range" min="0.3" max="0.9" step="0.05"
        value={settings.theme.button.intensity}
        oninput={(e) => setButtonIntensity(+e.currentTarget.value)}
      />
    </div>
  </section>

  <div class="sep"></div>

  <section>
    <span class="cap">Sichtbare Panele ({tab})</span>
    <div class="checks">
      {#each panelsForTab as p}
        <label>
          <input
            type="checkbox"
            checked={visiblePanels.includes(p.kind)}
            onchange={() => ontogglepanel(p.kind)}
          />
          {p.label}
        </label>
      {/each}
    </div>
  </section>

  <div class="sep"></div>

  <section>
    <span class="cap">Arbeitsplatz</span>
    <input type="text" value={settings.workplace} oninput={(e) => setWorkplace(e.currentTarget.value)} />
  </section>

  <button class="reset" onclick={onreset}>Reiter „{tab}" auf Standard zurücksetzen</button>
</div>

<style>
  .flyout {
    position: absolute;
    left: 48px;
    bottom: 12px;
    width: 260px;
    max-height: calc(100% - 24px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    z-index: 60;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
  }
  .x {
    width: 26px;
    height: 26px;
    padding: 0;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cap {
    font-size: 11px;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .row input[type="range"] {
    flex: 1;
  }
  input[type="color"] {
    width: 36px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }
  .checks {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  .checks label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  input[type="text"] {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 6px 8px;
    font-size: 13px;
  }
  .sep {
    height: 1px;
    background: var(--border);
  }
  button {
    background: var(--btn);
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 7px 10px;
    cursor: pointer;
  }
  button:hover {
    filter: brightness(1.15);
  }
  .reset {
    margin-top: 2px;
    font-size: 12px;
  }
</style>
