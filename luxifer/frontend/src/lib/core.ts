// Brücke zum Rust-Core über Tauri-Commands. Das Frontend hält KEINEN eigenen
// Wahrheits-Zustand — es holt den Zustand hier und zeichnet ihn nur.
import { invoke } from "@tauri-apps/api/core";

// Bildverarbeitungs-Modus (ADR 0004).
export type ImageMode =
  | "Grayscale"
  | "Threshold"
  | "Floyd"
  | "Jarvis"
  | "Stucki"
  | "Atkinson"
  | "Bayer"
  | "LaserRuns";

// Nicht-destruktive Bild-Parameter (spiegelt luxifer-core::ImageParams).
export interface ImageParams {
  mode: ImageMode;
  threshold: number;
  brightness: number;
  contrast: number;
  gamma: number;
  invert_editor: boolean;
  invert_laser: boolean;
}

// Spiegelt luxifer-core::Geo (serde-Enum, extern getaggt).
export type Geo =
  | { Rect: { x: number; y: number; w: number; h: number } }
  | { Ellipse: { cx: number; cy: number; rx: number; ry: number } }
  | { Polyline: { pts: [number, number][]; closed: boolean } }
  | {
      Image: {
        asset: string;
        x: number;
        y: number;
        w: number;
        h: number;
        params: ImageParams;
      };
    };

export interface Layer {
  name: string;
  color: [number, number, number];
  visible: boolean;
  enabled: boolean;
  active: boolean;
  locked: boolean;
  mode: "Cut" | "Fill" | "Raster" | "Image";
  speed_mm_s: number;
  power_pct: number;
  min_power_pct: number;
  air_assist: boolean;
  line_step_mm: number;
  passes: number;
  dpi: number;
  bidirectional: boolean;
}

export interface Shape {
  layer_id: number;
  geo: Geo;
  rotation: number;
  group_id?: number | null;
  speed_override?: number | null;
  power_override?: number | null;
}

// Metadaten des offenen Projekts (oder null, wenn namenlos).
export interface ProjectMeta {
  name: string;
  description: string;
  tags: string[];
  // ID der aktuellen Version (= was im Canvas ist).
  current_version: string;
}

// Was der Core dem Frontend zum Zeichnen gibt.
export interface Scene {
  layers: Layer[];
  shapes: Shape[];
  selected: number[];
  bed_w_mm: number;
  bed_h_mm: number;
  dirty: boolean;
  project: ProjectMeta | null;
}

export function rgb(color: [number, number, number]): string {
  const [r, g, b] = color;
  return `rgb(${r}, ${g}, ${b})`;
}

// ---- Command-Aufrufe -------------------------------------------------------

export const getScene = () => invoke<Scene>("get_scene");

export const addRect = (x: number, y: number, w: number, h: number) =>
  invoke<Scene>("add_rect", { x, y, w, h });

export const addEllipse = (cx: number, cy: number, rx: number, ry: number) =>
  invoke<Scene>("add_ellipse", { cx, cy, rx, ry });

export const addLine = (x1: number, y1: number, x2: number, y2: number) =>
  invoke<Scene>("add_line", { x1, y1, x2, y2 });

export const addPolyline = (pts: [number, number][], closed: boolean) =>
  invoke<Scene>("add_polyline", { pts, closed });

// ---- Bild-Import & -Bearbeitung (ADR 0004) --------------------------------

// Importiert ein Bild aus rohen Datei-Bytes (Frontend liest sie per <input file>).
export const importImageFile = (bytes: number[], name: string) =>
  invoke<Scene>("import_image_file", { bytes, name });

// Rendert ein Asset mit Parametern als PNG-Data-URL (Canvas/Editor-Vorschau).
export const imageRender = (asset: string, params: ImageParams, invert: boolean) =>
  invoke<string | null>("image_render", { asset, params, invert });

// Setzt die Bild-Parameter eines Bild-Shapes (Editor).
export const setImageParams = (index: number, params: ImageParams) =>
  invoke<Scene>("set_image_params", { index, params });

// Ein Asset eines Projekts (Anzeige im Browser).
export interface ProjectAsset {
  id: string;
  original_name: string;
  width: number;
  height: number;
  thumb: string | null;
}

// Assets eines Projekts (aus asset_refs) mit Metadaten + Vorschau.
export const projectAssets = (name: string) =>
  invoke<ProjectAsset[]>("project_assets", { name });

// Ein Eintrag des Formen-Katalogs (datengetriebene Galerie im Werkzeug-Panel).
// Spiegelt luxifer-core::ShapeInfo.
export interface ShapeInfo {
  id: string; // stabiler Bezeichner, z. B. "hex"
  label: string; // Anzeigename (deutsch)
  icon: string; // Icon-Name in Icon.svelte (= id)
}

export const shapeCatalog = () => invoke<ShapeInfo[]>("shape_catalog");

// Rein visuelle Vorschau-Punkte fuer das Polygon-Rubberband beim Aufziehen.
// WICHTIG: Das ist KEINE Fachlogik und keine Quelle der Wahrheit — die echte
// Form-Geometrie erzeugt ausschliesslich der Core (Command `add_polygon`).
// Diese Naeherung existiert nur, damit die Vorschau ohne Tauri-Roundtrip pro
// Mausbewegung fluessig bleibt; sie muss nur ungefaehr wie die Zielform aussehen.
export function polygonPreview(
  shape: string,
  cx: number,
  cy: number,
  r: number,
  rot: number,
): [number, number][] {
  r = Math.max(0.1, r);
  const start = -Math.PI / 2 + (rot * Math.PI) / 180;
  const ring = (n: number, radius: number, offset = 0) =>
    Array.from({ length: n }, (_, i): [number, number] => {
      const a = start + offset + (Math.PI * 2 * i) / n;
      return [cx + radius * Math.cos(a), cy + radius * Math.sin(a)];
    });
  const starRing = (pts: number, rin: number): [number, number][] =>
    Array.from({ length: pts * 2 }, (_, i): [number, number] => {
      const a = start + (Math.PI * i) / pts;
      const rr = i % 2 === 0 ? r : rin;
      return [cx + rr * Math.cos(a), cy + rr * Math.sin(a)];
    });
  switch (shape) {
    case "tri": return ring(3, r);
    case "quad": return ring(4, r);
    case "penta": return ring(5, r);
    case "hex": return ring(6, r);
    case "octa": return ring(8, r);
    case "star": return starRing(5, r * 0.382);
    case "sun": return starRing(12, r * 0.78);
    case "gear": {
      const teeth = 10, rin = r * 0.72, step = (Math.PI * 2) / teeth, q = step / 4;
      const out: [number, number][] = [];
      for (let i = 0; i < teeth; i++) {
        const c = start + step * i;
        for (const [off, rr] of [[-q, rin], [-q, r], [q, r], [q, rin]] as const) {
          const a = c + off;
          out.push([cx + rr * Math.cos(a), cy + rr * Math.sin(a)]);
        }
      }
      return out;
    }
    case "heart": {
      const SEGS = 40;
      const raw: [number, number][] = [];
      let max = 0;
      for (let i = 0; i < SEGS; i++) {
        const t = (Math.PI * 2 * i) / SEGS;
        const x = 16 * Math.sin(t) ** 3;
        const y = -(13 * Math.cos(t) - 5 * Math.cos(2 * t) - 2 * Math.cos(3 * t) - Math.cos(4 * t));
        max = Math.max(max, Math.abs(x), Math.abs(y));
        raw.push([x, y]);
      }
      const sc = max > 0 ? r / max : 1;
      const s = Math.sin((rot * Math.PI) / 180), co = Math.cos((rot * Math.PI) / 180);
      return raw.map(([x, y]): [number, number] => {
        const px = x * sc, py = y * sc;
        return [cx + px * co - py * s, cy + px * s + py * co];
      });
    }
    default: return ring(6, r);
  }
}

// Fügt eine parametrische Form (Katalog-`id`) mit Zentrum, Außenradius und
// Drehung (Grad) hinzu.
export const addPolygon = (
  shape: string,
  cx: number,
  cy: number,
  r: number,
  rot: number,
) => invoke<Scene>("add_polygon", { shape, cx, cy, r, rot });

export const activateColor = (color: [number, number, number]) =>
  invoke<Scene>("activate_color", { color });

export const selectAt = (x: number, y: number, tol: number, additive: boolean) =>
  invoke<Scene>("select_at", { x, y, tol, additive });

export const selectRect = (x1: number, y1: number, x2: number, y2: number) =>
  invoke<Scene>("select_rect", { x1, y1, x2, y2 });

export const moveSelected = (dx: number, dy: number) =>
  invoke<Scene>("move_selected", { dx, dy });

export const scaleSelected = (
  s: [number, number, number, number], // start box: x,y,w,h
  t: [number, number, number, number], // target box
) =>
  invoke<Scene>("scale_selected", {
    sx: s[0], sy: s[1], sw: s[2], sh: s[3],
    tx: t[0], ty: t[1], tw: t[2], th: t[3],
  });

export type AlignKind = "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom";
export type DistributeKind = "h" | "v";
export const align = (kind: AlignKind) => invoke<Scene>("align", { kind });
export const distribute = (kind: DistributeKind) => invoke<Scene>("distribute", { kind });

// "h" = horizontal spiegeln (links↔rechts), "v" = vertikal (oben↔unten).
export type MirrorAxis = "h" | "v";
export const mirror = (axis: MirrorAxis) => invoke<Scene>("mirror", { axis });

export const clearSelection = () => invoke<Scene>("clear_selection");

export const deleteSelected = () => invoke<Scene>("delete_selected");

export interface LayerParams {
  name: string;
  mode: "Cut" | "Fill" | "Raster" | "Image";
  speed_mm_s: number;
  power_pct: number;
  min_power_pct: number;
  passes: number;
  air_assist: boolean;
  line_step_mm: number;
  dpi: number;
  bidirectional: boolean;
}

export const setLayerParams = (index: number, p: LayerParams) =>
  invoke<Scene>("set_layer_params", { index, p });

export type LayerToggle = "visible" | "enabled" | "air_assist" | "locked";
export const toggleLayer = (index: number, field: LayerToggle) =>
  invoke<Scene>("toggle_layer", { index, field });

// Verschiebt einen Layer in der Brenn-Reihenfolge (ADR 0005 §0). Der Core
// remappt dabei alle shape.layer_id; ein Undo-Punkt entsteht nur bei Bewegung.
export const moveLayer = (from: number, to: number) =>
  invoke<Scene>("move_layer", { from, to });

// ---- Laser-Preview (ADR 0005) ---------------------------------------------

// Art eines Bewegungssegments. "Travel" = Leerfahrt (Laser aus).
export type MoveKind = "Cut" | "Fill" | "Raster" | "Travel";

// Ein Bewegungssegment der Vorschau in mm, in Ausführungsreihenfolge.
export interface PreviewMove {
  from: [number, number];
  to: [number, number];
  kind: MoveKind;
  layer_id: number;
  seq: number;
}

// Ein Bild-Layer als Textur (ADR 0008 §2): Pixel als Base64 (1 Byte/Texel,
// 255 = gebrannt) + Maße + Tisch-Box in mm. Das Frontend lädt sie als GPU-Textur.
export interface RasterTexture {
  pixels_b64: string;
  width: number;
  height: number;
  rect: [number, number, number, number]; // x, y, w, h in mm
}

// Die komplette Laser-Vorschau (abgeleitet aus dem JobPlan im Core).
export interface JobPreview {
  moves: PreviewMove[];
  rasters: RasterTexture[];
  bbox: [number, number, number, number] | null;
  total_len_mm: number;
}

// Leitet die Laser-Vorschau aus dem aktuellen Zustand ab (reine Ableitung,
// keine Mutation).
export const jobPreview = () => invoke<JobPreview>("job_preview");

// ---- Laser-Profile & gerätespezifische Aktionen (ADR 0007) -----------------

export type DriverKind = "Ruida" | "Grbl" | "MiniGrbl";

export type Connection =
  | { art: "netz"; ip: string; port: number | null }
  | { art: "seriell"; port: string; baud: number };

export interface ScanOffsetPoint {
  speed_mm_s: number;
  offset_mm: number;
}
export interface ScanOffsetCal {
  enabled: boolean;
  points: ScanOffsetPoint[];
}

export interface LaserProfile {
  id: string;
  name: string;
  kind: DriverKind;
  connection: Connection;
  bed_mm: [number, number];
  scan_offset: ScanOffsetCal;
}

export interface LaserRegistry {
  profiles: LaserProfile[];
  active_id: string | null;
}

export interface JobParamsDto {
  start_mode: "absolut" | "aktuell" | "ursprung";
  anchor: number;
}

export const laserList = () => invoke<LaserRegistry>("laser_list");
export const laserSave = (profile: LaserProfile) =>
  invoke<LaserRegistry>("laser_save", { profile });
export const laserDelete = (id: string) =>
  invoke<LaserRegistry>("laser_delete", { id });
export const laserSetActive = (id: string) =>
  invoke<LaserRegistry>("laser_set_active", { id });
export const laserActions = () => invoke<string[]>("laser_actions");
export const laserRunAction = (action: string, params: JobParamsDto) =>
  invoke<string>("laser_run_action", { action, params });
export const laserPing = () => invoke<boolean>("laser_ping");

export interface ExportDto {
  bytes: number[];
  filename: string;
}
export const laserExport = (params: JobParamsDto) =>
  invoke<ExportDto>("laser_export", { params });

export const laserJog = (dx: number, dy: number, speed: number) =>
  invoke<void>("laser_jog", { dx, dy, speed });
export const laserHome = (speed: number) => invoke<void>("laser_home", { speed });

export interface PositionDto {
  head: [number, number];
  origin: [number, number] | null;
}
export const laserPosition = () => invoke<PositionDto>("laser_position");

export const undo = () => invoke<Scene>("undo");
export const redo = () => invoke<Scene>("redo");

// ---- Projektverwaltung (ADR 0003) ------------------------------------------

// Eine Version (spiegelt luxifer-core::VersionInfo). Die aktuelle Version IST
// der Canvas (ADR 0003, 2026-07-08).
export interface VersionInfo {
  id: string;
  label: string; // Anzeigbare Nummer, z. B. „V3".
  created_at: string;
  note: string;
}

// Listeneintrag (linke Seite im Browser).
export interface ProjectInfo {
  name: string;
  tags: string[];
  description: string;
  modified_at: string;
}

// Volle Detailansicht (rechte Seite im Browser).
export interface ProjectDetail {
  name: string;
  description: string;
  tags: string[];
  created_at: string;
  modified_at: string;
  versions: VersionInfo[];
  current_version: string;
  asset_refs: string[];
}

export const newProject = () => invoke<Scene>("new_project");

// thumb ist ein PNG als Byte-Array (aus dem Offscreen-Canvas).
export const saveProject = (
  name: string,
  description: string,
  tags: string[],
  thumb: number[],
) => invoke<Scene>("save_project", { name, description, tags, thumbPng: thumb });

export const saveVersion = (note: string, thumb: number[]) =>
  invoke<Scene>("save_version", { note, thumbPng: thumb });

export const openProject = (name: string) =>
  invoke<Scene>("open_project", { name });

export const openVersion = (name: string, versionId: string) =>
  invoke<Scene>("open_version", { name, versionId });

// Löscht eine einzelne Version (letzte ist geschützt). Gibt die neue Scene
// zurück (bei gelöschter aktueller Version wird die vorherige geladen).
export const deleteVersion = (name: string, versionId: string) =>
  invoke<Scene>("delete_version", { name, versionId });

export const projectList = () => invoke<ProjectInfo[]>("project_list");

export const projectDetail = (name: string) =>
  invoke<ProjectDetail>("project_detail", { name });

// Thumbnail als Data-URL (oder null). Fuer Liste/Detail/Versionen.
export const projectThumb = (name: string) =>
  invoke<string | null>("project_thumb", { name });

export const versionThumb = (name: string, versionId: string) =>
  invoke<string | null>("version_thumb", { name, versionId });

export const projectDelete = (name: string) =>
  invoke<void>("project_delete", { name });

export const projectRename = (oldName: string, newName: string) =>
  invoke<void>("project_rename", { oldName, newName });

export const projectExport = (name: string, ziel: string) =>
  invoke<void>("project_export", { name, ziel });

export const swatchColors = () =>
  invoke<[number, number, number][]>("swatch_colors");

// ---- GUI-Settings (Panel-System, ADR 0002) ---------------------------------
// Spiegelt luxifer-core::ui_settings. Positionen sind Bruchteile (0…1) des
// Fensters, nie Pixel — das Snapping aufs Raster passiert erst beim Zeichnen.

export type Tab = "Projekt" | "Design" | "Laser" | "Monitor" | "Preview";

export type PanelKind =
  | "Werkzeuge"
  | "Ebenen"
  | "Farbpalette"
  | "Anordnen"
  | "Laser"
  | "JobStatus"
  | "Formen";

export interface PanelRect {
  x: number; // linke obere Ecke, Bruchteil 0…1
  y: number;
  w: number; // Ausdehnung, Bruchteil 0…1
  h: number;
  z: number; // Stapel-Reihenfolge bei Überlappung
}

export interface PanelPlacement {
  kind: PanelKind;
  rect: PanelRect;
}

export interface TabLayout {
  tab: Tab;
  panels: PanelPlacement[];
}

export interface ThemeColor {
  hue: [number, number, number];
  intensity: number; // geklemmt auf lesbaren Korridor (0.3…0.9)
}

export interface Theme {
  accent: ThemeColor;
  button: ThemeColor;
}

export interface UiSettings {
  version: number;
  workplace: string;
  theme: Theme;
  layouts: TabLayout[];
  last_project: string;
}

export const getUiSettings = () => invoke<UiSettings>("get_ui_settings");

// Speichert und gibt die aufgeräumten (geklemmten) Settings zurück.
export const saveUiSettings = (settings: UiSettings) =>
  invoke<UiSettings>("save_ui_settings", { settings });

// Setzt einen Reiter auf sein Standard-Layout zurück, speichert und gibt die
// aktualisierten Settings zurück (ADR §2).
export const resetTab = (tab: Tab) =>
  invoke<UiSettings>("reset_ui_tab", { tab });
