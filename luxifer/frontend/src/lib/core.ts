// Brücke zum Rust-Core über Tauri-Commands. Das Frontend hält KEINEN eigenen
// Wahrheits-Zustand — es holt den Zustand hier und zeichnet ihn nur.
import { invoke } from "@tauri-apps/api/core";

// Spiegelt luxifer-core::Geo (serde-Enum, extern getaggt).
export type Geo =
  | { Rect: { x: number; y: number; w: number; h: number } }
  | { Ellipse: { cx: number; cy: number; rx: number; ry: number } }
  | { Polyline: { pts: [number, number][]; closed: boolean } };

export interface Layer {
  name: string;
  color: [number, number, number];
  visible: boolean;
  enabled: boolean;
  active: boolean;
  locked: boolean;
  mode: "Cut" | "Fill" | "Raster";
  speed_mm_s: number;
  power_pct: number;
  min_power_pct: number;
  air_assist: boolean;
  line_step_mm: number;
  passes: number;
  dpi: number;
}

export interface Shape {
  layer_id: number;
  geo: Geo;
  rotation: number;
  group_id?: number | null;
  speed_override?: number | null;
  power_override?: number | null;
}

// Was der Core dem Frontend zum Zeichnen gibt.
export interface Scene {
  layers: Layer[];
  shapes: Shape[];
  selected: number[];
  bed_w_mm: number;
  bed_h_mm: number;
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
  mode: "Cut" | "Fill" | "Raster";
  speed_mm_s: number;
  power_pct: number;
  min_power_pct: number;
  passes: number;
  air_assist: boolean;
  line_step_mm: number;
  dpi: number;
}

export const setLayerParams = (index: number, p: LayerParams) =>
  invoke<Scene>("set_layer_params", { index, p });

export type LayerToggle = "visible" | "enabled" | "air_assist" | "locked";
export const toggleLayer = (index: number, field: LayerToggle) =>
  invoke<Scene>("toggle_layer", { index, field });

export const generateGcode = () => invoke<string>("generate_gcode");

export const ruidaPing = (ip: string) => invoke<boolean>("ruida_ping", { ip });
export const ruidaSend = (ip: string) => invoke<string>("ruida_send", { ip });

export const undo = () => invoke<Scene>("undo");
export const redo = () => invoke<Scene>("redo");

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
}

export const getUiSettings = () => invoke<UiSettings>("get_ui_settings");

// Speichert und gibt die aufgeräumten (geklemmten) Settings zurück.
export const saveUiSettings = (settings: UiSettings) =>
  invoke<UiSettings>("save_ui_settings", { settings });

// Setzt einen Reiter auf sein Standard-Layout zurück, speichert und gibt die
// aktualisierten Settings zurück (ADR §2).
export const resetTab = (tab: Tab) =>
  invoke<UiSettings>("reset_ui_tab", { tab });
