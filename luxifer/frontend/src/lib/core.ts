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
  | "JobStatus";

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
