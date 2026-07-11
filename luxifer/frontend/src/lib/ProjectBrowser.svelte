<script lang="ts">
  // Projekt-Browser (ADR 0003 §4): volle Body-Flaeche im Projekt-Reiter.
  // Links Suchfeld + Liste, rechts Details des gewaehlten Projekts inkl.
  // Versionsliste, Assets (vorbereitet), Charon-Status. Oben Aktionen.
  //
  // Der Browser haelt KEINEN Wahrheits-Zustand: Liste/Details holt er per
  // Command, Mutationen (speichern/laden/loeschen) meldet er an die App, die
  // den Scene-Zustand fuehrt.
  import { onMount } from "svelte";
  import * as core from "./core";
  import type { ProjectInfo, ProjectDetail, ProjectMeta } from "./core";
  import Icon from "./Icon.svelte";

  let {
    project,
    saveMode = false,
    onsave,
    onopen,
    onopenversion,
    ondeleteversion,
    onnew,
    ondeleted,
    onclosesavemode,
  }: {
    // Aktuell offenes Projekt (aus der Scene) — null, wenn namenlos.
    project: ProjectMeta | null;
    // Von der App gesetzt: Strg+S bei namenlosem Projekt oeffnet das Formular.
    saveMode?: boolean;
    // Speichern angestossen (Name/Beschreibung/Tags) — App ruft den Command.
    onsave: (name: string, description: string, tags: string[]) => Promise<void>;
    onopen: (name: string) => Promise<void>;
    onopenversion: (name: string, versionId: string) => Promise<void>;
    ondeleteversion: (name: string, versionId: string) => Promise<void>;
    onnew: () => Promise<void>;
    ondeleted: () => void;
    onclosesavemode: () => void;
  } = $props();

  let list = $state<ProjectInfo[]>([]);
  let search = $state("");
  let selected = $state<string | null>(null);
  let detail = $state<ProjectDetail | null>(null);
  let error = $state<string | null>(null);
  // Thumbnail-Data-URLs je Version (id → url).
  let verThumbs = $state<Record<string, string>>({});
  // Assets des gewählten Projekts (ADR 0004).
  let assets = $state<core.ProjectAsset[]>([]);

  // Versionen neueste-zuerst für die Anzeige (Grid + große Vorschau).
  const versionsNewestFirst = $derived([...(detail?.versions ?? [])].reverse());
  // ID der aktuellen Version (= was im Canvas ist). Kommt aus dem Detail bzw.,
  // wenn es das offene Projekt ist, aus dessen Scene-Meta.
  const currentId = $derived(detail?.current_version ?? "");
  // Große Vorschau zeigt IMMER die aktuelle Version (kein Hover/Pin mehr —
  // ADR 0003: die aktuelle Version ist der Canvas).
  const currentVer = $derived(detail?.versions.find((v) => v.id === currentId) ?? null);
  const previewThumb = $derived(currentId ? (verThumbs[currentId] ?? null) : null);

  // Formularfelder (Speichern / Details bearbeiten).
  let fName = $state("");
  let fDesc = $state("");
  let fTags = $state("");

  const filtered = $derived(
    list.filter((p) => {
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return (
        p.name.toLowerCase().includes(q) ||
        p.tags.some((t) => t.toLowerCase().includes(q)) ||
        p.description.toLowerCase().includes(q)
      );
    }),
  );

  async function refresh() {
    try {
      list = await core.projectList();
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  onMount(refresh);

  // Beim Wechsel in den Speichern-Modus: Formular mit offenem Projekt (oder leer)
  // vorbelegen und selektiertes Detail loeschen.
  $effect(() => {
    if (saveMode) {
      selected = null;
      detail = null;
      fName = project?.name ?? "";
      fDesc = project?.description ?? "";
      fTags = (project?.tags ?? []).join(", ");
    }
  });

  // Ist ein Projekt aktiv geladen und noch nichts ausgewaehlt, zeige direkt seine
  // Details (statt des Platzhalters). Manuelle Auswahl eines anderen Projekts
  // gewinnt; nur bei leerer Auswahl greift die Automatik.
  $effect(() => {
    const activeName = project?.name;
    if (activeName && !saveMode && selected === null) {
      selectProject(activeName);
    }
  });

  async function selectProject(name: string) {
    if (saveMode) onclosesavemode();
    selected = name;
    verThumbs = {};
    assets = [];
    try {
      detail = await core.projectDetail(name);
      fName = detail.name;
      fDesc = detail.description;
      fTags = detail.tags.join(", ");
      // Thumbnail je Version laden (jede Version hat genau eins).
      const thumbs: Record<string, string> = {};
      for (const v of detail.versions) {
        const t = await core.versionThumb(name, v.id);
        if (t) thumbs[v.id] = t;
      }
      verThumbs = thumbs;
      // Assets des Projekts laden (aus asset_refs).
      assets = await core.projectAssets(name);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  function parseTags(s: string): string[] {
    return s
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
  }

  async function doSave() {
    error = null;
    try {
      await onsave(fName, fDesc, parseTags(fTags));
      onclosesavemode();
      await refresh();
      if (fName) await selectProject(fName);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  async function doOpen() {
    if (!selected) return;
    try {
      await onopen(selected);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  async function doDelete() {
    if (!selected) return;
    if (!confirm(`Projekt „${selected}“ wirklich löschen?`)) return;
    try {
      await core.projectDelete(selected);
      if (project?.name === selected) ondeleted();
      selected = null;
      detail = null;
      await refresh();
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  async function doDeleteVersion(v: { id: string; label: string }) {
    if (!selected) return;
    if (!confirm(`Version ${v.label} wirklich löschen?`)) return;
    try {
      await ondeleteversion(selected, v.id);
      // Detail neu laden, damit Liste + aktuelle Version stimmen.
      await selectProject(selected);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  async function doRename() {
    if (!selected) return;
    const neu = prompt("Neuer Projektname:", selected);
    if (!neu || neu === selected) return;
    try {
      await core.projectRename(selected, neu);
      await refresh();
      await selectProject(neu);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  async function doExport() {
    if (!selected) return;
    // Einfacher Export: Zielpfad erfragen (Tauri-Dialog kommt spaeter).
    const ziel = prompt("Exportieren nach (voller Pfad zur .luxi):", `${selected}.luxi`);
    if (!ziel) return;
    try {
      await core.projectExport(selected, ziel);
    } catch (e) {
      error = core.errorMessage(e);
    }
  }

  // Kurzform „vor X" fuer ISO-Zeit. Grob, reicht fuer die Liste.
  function ago(iso: string): string {
    if (!iso) return "";
    const then = Date.parse(iso);
    if (isNaN(then)) return iso;
    const sec = Math.max(0, (Date.now() - then) / 1000);
    if (sec < 60) return "gerade eben";
    if (sec < 3600) return `vor ${Math.floor(sec / 60)} min`;
    if (sec < 86400) return `vor ${Math.floor(sec / 3600)} h`;
    return `vor ${Math.floor(sec / 86400)} Tg`;
  }
</script>

<div class="browser">
  <!-- Linke Spalte: Suche + Liste -->
  <aside class="list-col glass">
    <div class="list-head">
      <input class="search" placeholder="Projekte suchen…" bind:value={search} />
      <button class="new" onclick={onnew} title="Neues Projekt (Strg+N)">
        <Icon name="rect" /> Neu
      </button>
    </div>
    <div class="list">
      {#each filtered as p}
        <button class="item" class:active={selected === p.name} onclick={() => selectProject(p.name)}>
          <span class="item-name">{p.name}</span>
          <span class="item-meta">{ago(p.modified_at)}</span>
          {#if p.tags.length}
            <span class="item-tags">
              {#each p.tags as t}<span class="tag">{t}</span>{/each}
            </span>
          {/if}
        </button>
      {/each}
      {#if filtered.length === 0}
        <div class="empty">Keine Projekte{search ? " gefunden" : " – zeichne etwas und speichere mit Strg+S"}.</div>
      {/if}
    </div>
  </aside>

  <!-- Rechte Spalte: Details oder Speichern-Formular -->
  <section class="detail-col glass">
    {#if error}<div class="err">{error}</div>{/if}

    {#if saveMode}
      <h2>Projekt speichern</h2>
      <div class="form">
        <label>Name<input bind:value={fName} placeholder="Projektname" /></label>
        <label>Beschreibung<textarea bind:value={fDesc} rows="3"></textarea></label>
        <label>Tags (Komma-getrennt)<input bind:value={fTags} placeholder="deko, rund" /></label>
        <div class="actions">
          <button class="primary" onclick={doSave} disabled={!fName.trim()}>Speichern</button>
          <button onclick={onclosesavemode}>Abbrechen</button>
        </div>
      </div>
    {:else if detail}
      <!-- Kopf: Detailleiste (links) + große Vorschau der aktuellen Version. -->
      <div class="head-grid">
        <div class="detail-panel">
          <div class="detail-head">
            <h2>{detail.name}</h2>
            <span class="charon" title="Charon-Sync ist noch nicht angebunden">● offline – nicht verbunden</span>
          </div>
          <dl class="meta">
            <div><dt>Erstellt</dt><dd>{ago(detail.created_at)}</dd></div>
            <div><dt>Geändert</dt><dd>{ago(detail.modified_at)}</dd></div>
            <div><dt>Versionen</dt><dd>{detail.versions.length}</dd></div>
          </dl>
          <label class="edit">Beschreibung<textarea bind:value={fDesc} rows="2"></textarea></label>
          <label class="edit">Tags<input bind:value={fTags} /></label>
          <div class="actions">
            <button class="primary" onclick={doOpen}>Laden</button>
            <button onclick={() => onsave(fName, fDesc, parseTags(fTags)).then(refresh)}>Speichern</button>
            <button onclick={doRename}>Umbenennen</button>
            <button onclick={doExport}>Export</button>
            <button class="danger" onclick={doDelete}>Löschen</button>
          </div>
        </div>

        <!-- Große Vorschau: immer die aktuelle Version (= der Canvas). -->
        <div class="preview-col">
          <div class="thumb main">
            {#if previewThumb}
              <img src={previewThumb} alt="Vorschau der aktuellen Version" />
            {:else}
              <span class="no-thumb">Keine Vorschau – einmal speichern.</span>
            {/if}
            {#if currentVer}<span class="cur-badge">Aktuell · {currentVer.label}</span>{/if}
          </div>
          <div class="preview-label">Aktueller Stand{currentVer ? ` (${currentVer.label})` : ""}</div>
        </div>
      </div>

      <!-- Versions-Grid: jede Version eine Card, die aktuelle hervorgehoben. -->
      <div class="section-head">
        <h3>Versionen</h3>
        <span class="section-hint">Shift+Strg+S legt eine neue Version an · Laden setzt sie als aktuell</span>
      </div>
      <div class="vgrid">
        {#each versionsNewestFirst as v (v.id)}
          <div class="vcard" class:current={v.id === currentId}>
            <div class="vthumb">
              {#if verThumbs[v.id]}
                <img src={verThumbs[v.id]} alt={`Vorschau ${v.label}`} />
              {:else}
                <span class="no-thumb small">—</span>
              {/if}
              <span class="vlabel-badge">{v.label}</span>
              {#if v.id === currentId}<span class="vcur-tag">aktuell</span>{/if}
            </div>
            <div class="vmeta">
              <span class="vtime">{ago(v.created_at)}{v.note ? ` · ${v.note}` : ""}</span>
              <div class="vacts">
                {#if v.id !== currentId}
                  <button class="ver-load" onclick={() => onopenversion(detail!.name, v.id)}>Laden</button>
                {/if}
                {#if detail.versions.length > 1}
                  <button class="danger sm" onclick={() => doDeleteVersion(v)}>Löschen</button>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>

      <div class="section-head"><h3>Assets</h3></div>
      {#if assets.length}
        <div class="agrid">
          {#each assets as a (a.id)}
            <div class="acard" title={a.original_name}>
              <div class="athumb">
                {#if a.thumb}
                  <img src={a.thumb} alt={a.original_name} />
                {:else}
                  <span class="no-thumb small">—</span>
                {/if}
              </div>
              <div class="ameta">
                <span class="aname">{a.original_name || a.id}</span>
                <span class="adim">{a.width}×{a.height}</span>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="assets-box">Keine – Bilder/Fonts/DXF/SVG folgen mit dem Import.</div>
      {/if}
    {:else}
      <div class="placeholder">
        <p>Wähle links ein Projekt, um Details zu sehen.</p>
        <p class="hint">Strg+S speichert den aktuellen Entwurf · Shift+Strg+S legt eine Version an.</p>
      </div>
    {/if}
  </section>
</div>

<style>
  /* Deckender Hintergrund ueber die ganze Flaeche: das Canvas dahinter darf
     nicht durchscheinen (stoert die Ansicht). */
  .browser {
    position: absolute;
    inset: 0;
    background: var(--bg, #16171b);
    padding: 64px 12px 12px 12px;
    display: grid;
    grid-template-columns: minmax(220px, 320px) 1fr;
    gap: 12px;
    z-index: 40;
  }
  /* Panels im Browser sind deckend (kein Frosted-Glas ueber dem Canvas). */
  .glass {
    border-radius: 12px;
    overflow: hidden;
    background: #1c1e24;
    border: 1px solid var(--border);
  }
  .list-col { display: flex; flex-direction: column; }
  .list-head {
    display: flex;
    gap: 6px;
    padding: 10px;
    border-bottom: 1px solid var(--border);
  }
  .search {
    flex: 1;
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 6px 8px;
    font-size: 13px;
  }
  .new {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--btn);
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 6px 10px;
    cursor: pointer;
    font-size: 13px;
    white-space: nowrap;
  }
  .list { overflow-y: auto; padding: 6px; display: flex; flex-direction: column; gap: 4px; }
  .item {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 8px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 8px 10px;
    cursor: pointer;
    color: var(--text);
  }
  .item:hover { background: rgba(255, 255, 255, 0.05); }
  .item.active { background: rgba(255, 255, 255, 0.08); border-color: var(--accent); }
  .item-name { font-weight: 600; font-size: 13px; }
  .item-meta { color: var(--muted); font-size: 11px; align-self: center; }
  .item-tags { grid-column: 1 / -1; display: flex; gap: 4px; flex-wrap: wrap; margin-top: 2px; }
  .tag {
    font-size: 10px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    padding: 1px 6px;
    color: var(--muted);
  }
  .detail-col { padding: 16px 18px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .detail-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
  /* Kopf: Detailleiste links (flexibel), große Vorschau rechts (fest). */
  .head-grid {
    display: grid;
    grid-template-columns: 1fr minmax(240px, 320px);
    gap: 20px;
    align-items: start;
  }
  .detail-panel { min-width: 0; display: flex; flex-direction: column; }
  .preview-col { display: flex; flex-direction: column; gap: 6px; }
  .preview-label { font-size: 12px; color: var(--muted); text-align: center; }
  h2 { margin: 0; font-size: 18px; }
  h3 { margin: 0; font-size: 13px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.5px; }
  .charon { font-size: 11px; color: #c98b3f; }
  .meta { display: flex; gap: 24px; margin: 0 0 12px; }
  .meta div { display: flex; flex-direction: column; }
  .meta dt { font-size: 10px; color: var(--muted); text-transform: uppercase; }
  .meta dd { margin: 0; font-size: 13px; }
  /* Abschnittsköpfe (Versionen, Assets). */
  .section-head { display: flex; align-items: baseline; gap: 12px; }
  .section-hint { font-size: 12px; color: var(--muted); }
  .form, .edit { display: flex; flex-direction: column; gap: 8px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  input, textarea {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 7px 9px;
    font-size: 13px;
    font-family: inherit;
    resize: vertical;
  }
  .edit { margin-top: 8px; }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 14px; }
  button {
    background: var(--btn);
    color: var(--text);
    border: none;
    border-radius: 6px;
    padding: 7px 12px;
    cursor: pointer;
    font-size: 13px;
  }
  button:hover { filter: brightness(1.15); }
  button:disabled { opacity: 0.5; cursor: default; }
  .primary { background: var(--accent); color: white; }
  .danger { background: #6b2b2b; color: #ffb4b4; }
  /* Grosse Vorschau rechts: immer die aktuelle Version (= der Canvas). */
  .thumb.main {
    position: relative;
    width: 100%;
    aspect-ratio: 4 / 3;
    background: #141518;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .thumb.main img { width: 100%; height: 100%; object-fit: contain; }
  .no-thumb { color: var(--muted); font-size: 12px; padding: 0 12px; text-align: center; }
  .no-thumb.small { font-size: 18px; }
  .cur-badge {
    position: absolute;
    top: 8px;
    left: 8px;
    font-size: 10px;
    letter-spacing: 1px;
    text-transform: uppercase;
    font-weight: 600;
    color: #fff;
    background: var(--accent);
    padding: 3px 9px;
    border-radius: 20px;
  }

  /* Versions-Grid: jede Version eine Card, die aktuelle hervorgehoben. */
  .vgrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 14px;
  }
  .vcard {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 12px;
    overflow: hidden;
    background: #16171b;
    transition: transform 0.1s ease, border-color 0.16s ease;
  }
  .vcard:hover { transform: translateY(-2px); border-color: var(--accent); }
  .vcard.current {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent), 0 0 18px -6px var(--accent);
  }
  .vthumb {
    position: relative;
    aspect-ratio: 4 / 3;
    background: #141518;
    display: flex;
    align-items: center;
    justify-content: center;
    border-bottom: 1px solid var(--border);
  }
  .vthumb img { width: 100%; height: 100%; object-fit: contain; }
  .vlabel-badge {
    position: absolute;
    top: 7px;
    left: 7px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.5px;
    background: rgba(0, 0, 0, 0.6);
    color: var(--text);
    padding: 2px 8px;
    border-radius: 6px;
  }
  .vcard.current .vlabel-badge { background: var(--accent); color: #fff; }
  .vcur-tag {
    position: absolute;
    top: 7px;
    right: 7px;
    font-size: 9px;
    letter-spacing: 1px;
    text-transform: uppercase;
    font-weight: 600;
    background: var(--accent);
    color: #fff;
    padding: 2px 7px;
    border-radius: 20px;
  }
  .vmeta { display: flex; align-items: center; gap: 8px; padding: 8px 10px; }
  .vtime { flex: 1; font-size: 12px; color: var(--muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vacts { display: flex; gap: 6px; }
  .ver-load { padding: 4px 10px; font-size: 12px; }
  .danger.sm { padding: 4px 9px; font-size: 12px; }
  /* Assets-Platzhalter. */
  .assets-box {
    padding: 18px;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    background: rgba(0, 0, 0, 0.15);
  }
  /* Asset-Grid (importierte Bilder). */
  .agrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 12px;
  }
  .acard {
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: #16171b;
  }
  .athumb {
    aspect-ratio: 4 / 3;
    background: #141518;
    display: flex;
    align-items: center;
    justify-content: center;
    border-bottom: 1px solid var(--border);
  }
  .athumb img { width: 100%; height: 100%; object-fit: contain; }
  .ameta { display: flex; flex-direction: column; gap: 2px; padding: 6px 8px; }
  .aname {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .adim { font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .empty { color: var(--muted); font-size: 13px; padding: 10px; text-align: center; }
  .placeholder { color: var(--muted); display: flex; flex-direction: column; gap: 8px; margin-top: 40px; align-items: center; }
  .hint { font-size: 12px; }
  .err { background: #331e1e; color: #e5645d; padding: 6px 10px; border-radius: 6px; margin-bottom: 10px; font-size: 13px; }
</style>
