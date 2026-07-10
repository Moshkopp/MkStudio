<script lang="ts">
  // Zentrales Einstellungs-Modal (Zahnrad oben rechts). Sektions-Navigation;
  // die Laser-Verwaltung (ADR 0007) ist die erste Sektion, weitere folgen.
  import type { LaserRegistry, LaserProfile, UiSettings } from "./core";
  import LaserSettingsContent from "./LaserSettingsContent.svelte";

  let {
    registry,
    settings,
    onsave,
    ondelete,
    onsavesettings,
    onclose,
  }: {
    registry: LaserRegistry | null;
    settings: UiSettings | null;
    onsave: (profile: LaserProfile) => void;
    ondelete: (id: string) => void;
    onsavesettings: (settings: UiSettings) => void;
    onclose: () => void;
  } = $props();

  type Section = "laser" | "oberflaeche";
  let section = $state<Section>("laser");

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
        <button class="sec" class:on={section === "laser"} onclick={() => (section = "laser")}>
          Laser
        </button>
        <button
          class="sec"
          class:on={section === "oberflaeche"}
          onclick={() => (section = "oberflaeche")}
        >
          Oberfläche
        </button>
      </nav>

      <!-- Sektions-Inhalt -->
      <div class="content">
        {#if section === "laser"}
          <LaserSettingsContent {registry} {onsave} {ondelete} />
        {:else}
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
</style>
