<script lang="ts">
  // Zentrales Einstellungs-Modal (Zahnrad oben rechts). Sektions-Navigation;
  // die Laser-Verwaltung (ADR 0007) ist die erste Sektion, weitere folgen.
  import type { LaserRegistry, LaserProfile, UiSettings, AppVersion } from "./core";
  import LaserSettingsContent from "./LaserSettingsContent.svelte";
  import logoUrl from "../assets/logo.png";

  let {
    registry,
    settings,
    version,
    onsave,
    ondelete,
    onsavesettings,
    onclose,
  }: {
    registry: LaserRegistry | null;
    settings: UiSettings | null;
    version: AppVersion;
    onsave: (profile: LaserProfile) => void;
    ondelete: (id: string) => void;
    onsavesettings: (settings: UiSettings) => void;
    onclose: () => void;
  } = $props();

  type Section = "ui" | "laser" | "backup" | "hilfe" | "about";
  const SECTIONS: [Section, string][] = [
    ["ui", "Oberfläche"],
    ["laser", "Laser"],
    ["backup", "Backup / Restore"],
    ["hilfe", "Hilfe"],
    ["about", "Über"],
  ];
  let section = $state<Section>("ui");

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

  function save(next: UiSettings) {
    onsavesettings(next);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />
<div class="overlay" onclick={onclose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="modal"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <header>
      <h2>Einstellungen</h2>
      <button class="x" onclick={onclose} aria-label="Schließen">✕</button>
    </header>

    <div class="split">
      <!-- Sektions-Navigation -->
      <nav class="sections">
        {#each SECTIONS as [id, label] (id)}
          <button class="sec" class:on={section === id} onclick={() => (section = id)}>
            {label}
          </button>
        {/each}
      </nav>

      <!-- Sektions-Inhalt -->
      <div class="content">
        {#if section === "laser"}
          <LaserSettingsContent {registry} {onsave} {ondelete} />
        {:else if section === "ui"}
          {#if settings}
            <div class="ui">
              <label class="field">
                Arbeitsplatz
                <input
                  type="text"
                  value={settings.workplace}
                  oninput={(e) => save({ ...settings, workplace: e.currentTarget.value })}
                />
              </label>

              <label class="field">
                Raster-Größe (mm)
                <div class="row">
                  <input
                    type="range"
                    min="1"
                    max="200"
                    step="1"
                    value={settings.grid_size_mm}
                    oninput={(e) => save({ ...settings, grid_size_mm: +e.currentTarget.value })}
                  />
                  <input
                    class="num"
                    type="number"
                    min="1"
                    max="500"
                    value={settings.grid_size_mm}
                    oninput={(e) => save({ ...settings, grid_size_mm: +e.currentTarget.value })}
                  />
                </div>
              </label>

              <label class="check">
                <input
                  type="checkbox"
                  checked={settings.show_splash}
                  onchange={(e) => save({ ...settings, show_splash: e.currentTarget.checked })}
                />
                Splash-Screen beim Start anzeigen
              </label>

              <label class="field" class:disabled={!settings.show_splash}>
                Splash-Dauer: {(settings.splash_ms / 1000).toFixed(1)} s
                <input
                  type="range"
                  min="0"
                  max="5000"
                  step="100"
                  disabled={!settings.show_splash}
                  value={settings.splash_ms}
                  oninput={(e) => save({ ...settings, splash_ms: +e.currentTarget.value })}
                />
              </label>

              <label class="field">
                Akzentfarbe
                <div class="row">
                  <input
                    type="color"
                    value={hex(settings.theme.accent.hue)}
                    oninput={(e) => save({
                      ...settings,
                      theme: {
                        ...settings.theme,
                        accent: { ...settings.theme.accent, hue: fromHex(e.currentTarget.value) },
                      },
                    })}
                  />
                  <input
                    type="range"
                    min="0.3"
                    max="0.9"
                    step="0.05"
                    value={settings.theme.accent.intensity}
                    oninput={(e) => save({
                      ...settings,
                      theme: {
                        ...settings.theme,
                        accent: { ...settings.theme.accent, intensity: +e.currentTarget.value },
                      },
                    })}
                  />
                </div>
              </label>

              <label class="field">
                Button-Farbe
                <div class="row">
                  <input
                    type="color"
                    value={hex(settings.theme.button.hue)}
                    oninput={(e) => save({
                      ...settings,
                      theme: {
                        ...settings.theme,
                        button: { ...settings.theme.button, hue: fromHex(e.currentTarget.value) },
                      },
                    })}
                  />
                  <input
                    type="range"
                    min="0.3"
                    max="0.9"
                    step="0.05"
                    value={settings.theme.button.intensity}
                    oninput={(e) => save({
                      ...settings,
                      theme: {
                        ...settings.theme,
                        button: { ...settings.theme.button, intensity: +e.currentTarget.value },
                      },
                    })}
                  />
                </div>
              </label>
            </div>
          {/if}
        {:else if section === "backup"}
          <div class="stub">
            <h3>Backup / Restore</h3>
            <p>Projekte und Einstellungen sichern und wiederherstellen.</p>
            <p class="soon">Noch nicht verfügbar — folgt.</p>
          </div>
        {:else if section === "hilfe"}
          <div class="stub">
            <h3>Hilfe</h3>
            <p>Kurzanleitung, Tastenkürzel und Support.</p>
            <p class="soon">Noch nicht verfügbar — folgt.</p>
          </div>
        {:else if section === "about"}
          <div class="about">
            <img class="logo" src={logoUrl} alt="LuxiFer" />
            <div class="app-name">LuxiFer</div>
            <div class="ver">
              {version.version || "—"}{#if version.commit && version.commit !== "-"} · {version.commit}{/if}
            </div>
            <p class="tag">Offline-first Laser-Steuerung.</p>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    z-index: 1000;
  }
  .modal {
    width: min(760px, 92vw);
    max-height: 86vh;
    background: var(--panel, #1c1f26);
    border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
    border-radius: 14px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .x {
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 15px;
  }
  .split {
    display: grid;
    grid-template-columns: 150px 1fr;
    min-height: 0;
    flex: 1;
  }
  .sections {
    border-right: 1px solid var(--border);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sec {
    text-align: left;
    padding: 8px 12px;
    border-radius: 8px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
  }
  .sec:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .sec.on {
    background: rgba(255, 255, 255, 0.08);
    border-color: var(--accent);
  }
  .content {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .ui {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
    color: var(--muted);
    font-size: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .field.disabled {
    opacity: 0.45;
  }
  input[type="text"] {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 7px;
    padding: 7px 9px;
  }
  input[type="color"] {
    width: 42px;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: none;
    cursor: pointer;
  }
  input[type="range"] { flex: 1; }
  .num {
    width: 68px;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 7px;
    padding: 6px 8px;
    font-variant-numeric: tabular-nums;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
  }
  .check input { width: 15px; height: 15px; cursor: pointer; }

  /* Stub-Sektionen (Backup, Hilfe) */
  .stub {
    padding: 24px;
    color: var(--muted);
  }
  .stub h3 { margin: 0 0 8px; color: var(--text); font-size: 15px; }
  .stub p { margin: 4px 0; font-size: 13px; }
  .stub .soon {
    margin-top: 14px;
    display: inline-block;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 12px;
    opacity: 0.8;
  }

  /* Über-Sektion */
  .about {
    padding: 28px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
  }
  .about .logo {
    width: 120px;
    height: auto;
    filter: drop-shadow(0 0 20px rgba(240, 150, 40, 0.3));
  }
  .about .app-name { font-size: 22px; font-weight: 700; letter-spacing: 1.5px; color: var(--text); }
  .about .ver { font-size: 13px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .about .tag { font-size: 13px; color: var(--muted); margin: 6px 0 0; }
</style>
