<script lang="ts">
  // Text-Werkzeug: Text + Font + Größe → Vektorpfade im Canvas (Text→Pfad).
  // Der Core erzeugt die Glyph-Konturen; hier nur Eingabe + Font-Auswahl.
  import { onMount } from "svelte";
  import { errorMessage, listFonts, textPreview, uploadFont, type FontInfo } from "./core";

  import type { TextMeta } from "./core";

  let {
    initial = null,
    oninsert,
    onclose,
  }: {
    // Vorbelegung beim Editieren (Doppelklick auf einen Text-Block).
    initial?: TextMeta | null;
    oninsert: (text: string, fontPath: string, sizeMm: number) => void;
    onclose: () => void;
  } = $props();

  /* svelte-ignore state_referenced_locally */
  let text = $state(initial?.text ?? "");
  /* svelte-ignore state_referenced_locally */
  let sizeMm = $state(initial?.size_mm ?? 20);
  let fonts = $state<FontInfo[]>([]);
  /* svelte-ignore state_referenced_locally */
  let fontPath = $state(initial?.font_path ?? "");
  let error = $state("");

  // Eigene Fonts (App-Ordner) zuerst, Systemfonts als eingeklappte Gruppe —
  // sonst gehen die eigenen zwischen hunderten Systemschriften unter.
  const ownFonts = $derived(fonts.filter((f) => f.path.includes("/luxifer/Fonts/")));
  const sysFonts = $derived(fonts.filter((f) => !f.path.includes("/luxifer/Fonts/")));

  onMount(async () => {
    fonts = await listFonts();
    if (fontPath) return; // Edit-Modus: Font des Blocks behalten
    // Default: erster eigener Font, sonst ein gut lesbarer System-Font.
    const own = fonts.find((f) => f.path.includes("/luxifer/Fonts/"));
    const pref = fonts.find((f) => /dejavusans$|liberationsans-regular|arial$/i.test(f.name));
    fontPath = (own ?? pref ?? fonts[0])?.path ?? "";
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

  // Live-Vorschau der gewählten Schrift: Core liefert die Konturen, das
  // kleine Canvas zeichnet sie nur (entprellt; nur der letzte Aufruf zählt).
  let previewCanvas = $state<HTMLCanvasElement | null>(null);
  let previewToken = 0;
  $effect(() => {
    const t = text.trim() || "AaBb 123";
    const fp = fontPath;
    const sz = sizeMm;
    if (!fp || !previewCanvas) return;
    const token = ++previewToken;
    textPreview(t, fp, sz)
      .then((contours) => {
        if (token !== previewToken || !previewCanvas) return;
        const ctx = previewCanvas.getContext("2d");
        if (!ctx) return;
        const W = previewCanvas.width;
        const H = previewCanvas.height;
        ctx.clearRect(0, 0, W, H);
        // Bounding-Box der Konturen → einpassen.
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        for (const [pts] of contours)
          for (const [x, y] of pts) {
            minX = Math.min(minX, x); minY = Math.min(minY, y);
            maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
          }
        if (!isFinite(minX)) return;
        const pad = 8;
        const sc = Math.min((W - 2 * pad) / (maxX - minX || 1), (H - 2 * pad) / (maxY - minY || 1));
        ctx.strokeStyle = "#dfe3ea";
        ctx.lineWidth = 1.2;
        for (const [pts, closed] of contours) {
          ctx.beginPath();
          pts.forEach(([x, y], i) => {
            const sx = pad + (x - minX) * sc;
            const sy = pad + (y - minY) * sc;
            if (i === 0) ctx.moveTo(sx, sy);
            else ctx.lineTo(sx, sy);
          });
          if (closed) ctx.closePath();
          ctx.stroke();
        }
      })
      .catch(() => {});
  });

  // Eigenen Font installieren (TTF/OTF → App-Fonts-Ordner) und auswählen.
  let fontFile = $state<HTMLInputElement | null>(null);
  async function onFontFile(ev: Event) {
    const input = ev.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const path = await uploadFont(bytes, file.name);
      fonts = await listFonts();
      fontPath = path;
      error = "";
    } catch (e) {
      error = `Font nicht installierbar: ${errorMessage(e)}`;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="button" tabindex="-1">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>{initial ? "Text bearbeiten" : "Text einfügen"}</h3>

    <label>
      Text
      <!-- svelte-ignore a11y_autofocus -->
      <textarea rows="3" bind:value={text} autofocus placeholder="Gravur-Text …"></textarea>
    </label>

    <div class="row">
      <label class="grow">
        Schriftart
        <select bind:value={fontPath}>
          {#if ownFonts.length > 0}
            <optgroup label="Eigene Fonts">
              {#each ownFonts as f (f.path)}
                <option value={f.path}>{f.name}</option>
              {/each}
            </optgroup>
            <optgroup label="Systemschriften">
              {#each sysFonts as f (f.path)}
                <option value={f.path}>{f.name}</option>
              {/each}
            </optgroup>
          {:else}
            {#each fonts as f (f.path)}
              <option value={f.path}>{f.name}</option>
            {/each}
          {/if}
        </select>
      </label>
      <label class="size">
        Größe (mm)
        <input type="number" min="1" max="500" step="1" bind:value={sizeMm} />
      </label>
    </div>

    <div class="preview"><canvas bind:this={previewCanvas} width="420" height="90"></canvas></div>

    {#if error}<p class="err">{error}</p>{/if}

    <div class="actions">
      <button class="ghost" onclick={() => fontFile?.click()} title="TTF/OTF in den App-Fonts-Ordner installieren">
        Font installieren…
      </button>
      <input type="file" accept=".ttf,.otf" bind:this={fontFile} onchange={onFontFile} hidden />
      <button class="ghost" onclick={onclose}>Abbrechen</button>
      <button class="primary" onclick={insert}>{initial ? "Übernehmen" : "Einfügen"}</button>
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
  .preview {
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid var(--border);
    border-radius: 8px;
    display: flex;
    justify-content: center;
  }
  .preview canvas {
    display: block;
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
