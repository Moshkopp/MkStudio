<script lang="ts">
  // Text-Werkzeug: Text + Font + Größe → Vektorpfade im Canvas (Text→Pfad).
  // Der Core erzeugt die Glyph-Konturen; hier nur Eingabe + Font-Auswahl.
  import { onMount } from "svelte";
  import { listFonts, type FontInfo } from "./core";

  let {
    oninsert,
    onclose,
  }: {
    oninsert: (text: string, fontPath: string, sizeMm: number) => void;
    onclose: () => void;
  } = $props();

  let text = $state("");
  let sizeMm = $state(20);
  let fonts = $state<FontInfo[]>([]);
  let fontPath = $state("");
  let error = $state("");

  onMount(async () => {
    fonts = await listFonts();
    // Sinnvoller Default: ein gut lesbarer Standard-Font, sonst der erste.
    const pref = fonts.find((f) => /dejavusans$|liberationsans-regular|arial$/i.test(f.name));
    fontPath = (pref ?? fonts[0])?.path ?? "";
  });

  function insert() {
    if (!text.trim()) {
      error = "Bitte Text eingeben.";
      return;
    }
    if (!fontPath) {
      error = "Kein Font gefunden.";
      return;
    }
    oninsert(text, fontPath, sizeMm);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="button" tabindex="-1">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>Text einfügen</h3>

    <label>
      Text
      <!-- svelte-ignore a11y_autofocus -->
      <textarea rows="3" bind:value={text} autofocus placeholder="Gravur-Text …"></textarea>
    </label>

    <div class="row">
      <label class="grow">
        Schriftart
        <select bind:value={fontPath}>
          {#each fonts as f (f.path)}
            <option value={f.path}>{f.name}</option>
          {/each}
        </select>
      </label>
      <label class="size">
        Größe (mm)
        <input type="number" min="1" max="500" step="1" bind:value={sizeMm} />
      </label>
    </div>

    {#if error}<p class="err">{error}</p>{/if}

    <div class="actions">
      <button class="ghost" onclick={onclose}>Abbrechen</button>
      <button class="primary" onclick={insert}>Einfügen</button>
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
    width: min(460px, 92vw);
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 18px 50px -12px rgba(0, 0, 0, 0.7);
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12px;
    color: var(--muted);
  }
  textarea,
  select,
  input {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 7px 8px;
    font-size: 13px;
    font-family: inherit;
    resize: vertical;
  }
  .row {
    display: flex;
    gap: 10px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .size {
    width: 90px;
  }
  .err {
    margin: 0;
    color: #ff5c62;
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
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
</style>
