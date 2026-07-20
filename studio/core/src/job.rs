//! Geräteunabhängige Job-Repräsentation (`JobPlan`) und der `MachineDriver`-Trait.
//!
//! Der Core wandelt Shapes + Layer in einen `JobPlan` (Bewegungen in mm, nach
//! Layer gruppiert). Konkrete Treiber (Ruida, GRBL, miniGRBL) übersetzen den
//! Plan in ihr Format — der Core kennt selbst KEIN Gerät (ADR 0001).

use crate::geometry::{rotate_point, Geo, Pt};
use crate::model::{Layer, LayerMode, Shape};
use crate::raster::{
    raster_rows, raster_texture, Placement, RasterImage, RasterRow, RasterTexture,
};
use crate::scanline::{fill_compound_segments, Contour, FillSegment};

/// Ein zusammenhängender Pfad in mm (Polygonzug). `closed` = Kontur schließt sich.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub points: Vec<Pt>,
    pub closed: bool,
}

/// Die auf einem Layer auszuführende Arbeit (geräteunabhängig).
#[derive(Debug, Clone, PartialEq)]
pub enum LayerWork {
    /// Konturen abfahren (Cut/Gravur-Linien).
    Cut { paths: Vec<Path> },
    /// Fläche mit horizontalen Linien füllen (Even-Odd-Scanline).
    Fill { segments: Vec<FillSegment> },
    /// Bild rastern: An/Aus-Zeilen aus einem geschwellten Graustufenbild
    /// (raster.rs). `rows` = die durchzubrennenden Runs (der Treiber fährt sie).
    /// `texture` = dieselbe Rasterung als Pixel für die Vorschau (ADR 0008 §2);
    /// aus denselben Daten, damit die Vorschau nicht 445k Segmente braucht.
    Raster {
        rows: Vec<RasterRow>,
        texture: Option<RasterTexture>,
    },
}

/// Ein Layer-Block des Jobs: Parameter + Arbeit. Referenziert den Original-Layer.
#[derive(Debug, Clone, PartialEq)]
pub struct JobLayer {
    pub layer_id: usize,
    /// Layerfarbe (manche Treiber, z. B. Ruida, kodieren sie in der Config).
    pub color: [u8; 3],
    pub speed_mm_s: f64,
    pub power_pct: f64,
    pub min_power_pct: f64,
    pub passes: u32,
    /// Luftunterstützung an? Geräteneutral — jeder Treiber, der Air-Assist
    /// kennt, schaltet danach (Ruida pro Layer, GRBL per M7/M9).
    pub air_assist: bool,
    /// Bidirektionales Rastern (Scan hin und zurück). Nur für Fill/Raster
    /// relevant; steuert im Treiber den Rückwärts-Scan (und damit, ob dessen
    /// geschwindigkeitsabhängige Reversal-Korrektur greift).
    pub bidirectional: bool,
    pub work: LayerWork,
}

impl JobLayer {
    /// Bounding-Box der Arbeit dieses Layers in mm (min_x, min_y, max_x, max_y).
    pub fn bbox(&self) -> Option<(f64, f64, f64, f64)> {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut any = false;
        let mut acc = |x: f64, y: f64| {
            any = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        };
        match &self.work {
            LayerWork::Cut { paths } => {
                for p in paths {
                    for &(x, y) in &p.points {
                        acc(x, y);
                    }
                }
            }
            LayerWork::Fill { segments } => {
                for s in segments {
                    acc(s.x0, s.y);
                    acc(s.x1, s.y);
                }
            }
            LayerWork::Raster { rows, .. } => {
                for r in rows {
                    for &(x0, x1) in &r.runs {
                        acc(x0, r.y);
                        acc(x1, r.y);
                    }
                }
            }
        }
        any.then_some((min_x, min_y, max_x, max_y))
    }
}

/// Der komplette, geräteunabhängige Job. Alle Maße in mm.
#[derive(Debug, Clone, PartialEq)]
pub struct JobPlan {
    pub layers: Vec<JobLayer>,
    /// Bounding-Box aller Geometrie (mm): (min_x, min_y, max_x, max_y).
    pub bbox: Option<(f64, f64, f64, f64)>,
}

impl JobPlan {
    /// Spiegelt einen im links-obigen Editor-Koordinatensystem aufgebauten Plan
    /// in das Koordinatensystem des Maschinenprofils.
    pub fn transformed_for_bed(&self, origin: crate::BedOrigin, bed: (f64, f64)) -> Self {
        let mut plan = self.clone();
        for layer in &mut plan.layers {
            match &mut layer.work {
                LayerWork::Cut { paths } => {
                    for path in paths {
                        for point in &mut path.points {
                            *point = origin.transform(point.0, point.1, bed);
                        }
                    }
                }
                LayerWork::Fill { segments } => {
                    for segment in segments {
                        let a = origin.transform(segment.x0, segment.y, bed);
                        let b = origin.transform(segment.x1, segment.y, bed);
                        segment.x0 = a.0.min(b.0);
                        segment.x1 = a.0.max(b.0);
                        segment.y = a.1;
                    }
                }
                LayerWork::Raster { rows, .. } => {
                    for row in rows {
                        row.y = origin.transform(0.0, row.y, bed).1;
                        for run in &mut row.runs {
                            let a = origin.transform(run.0, 0.0, bed).0;
                            let b = origin.transform(run.1, 0.0, bed).0;
                            *run = (a.min(b), a.max(b));
                        }
                    }
                }
            }
        }
        plan.bbox = bounding_box(&plan.layers);
        plan
    }

    /// Verschiebt alle geplanten Punkte um (dx, dy) mm. Reine Translation —
    /// keine Spiegelung, keine Skalierung.
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        let mut plan = self.clone();
        for layer in &mut plan.layers {
            match &mut layer.work {
                LayerWork::Cut { paths } => {
                    for path in paths {
                        for point in &mut path.points {
                            point.0 += dx;
                            point.1 += dy;
                        }
                    }
                }
                LayerWork::Fill { segments } => {
                    for segment in segments {
                        segment.x0 += dx;
                        segment.x1 += dx;
                        segment.y += dy;
                    }
                }
                LayerWork::Raster { rows, .. } => {
                    for row in rows {
                        row.y += dy;
                        for run in &mut row.runs {
                            run.0 += dx;
                            run.1 += dx;
                        }
                    }
                }
            }
        }
        plan.bbox = plan
            .bbox
            .map(|(x0, y0, x1, y1)| (x0 + dx, y0 + dy, x1 + dx, y1 + dy));
        plan
    }

    /// Legt den gewählten 3×3-Anker der Plan-BBox auf eine absolute Zielkoordinate
    /// (ADR 0020 §G: gespeicherter Nullpunkt als Jobreferenz). Leerer Plan bleibt
    /// unverändert.
    pub fn placed_with_anchor_at(&self, anchor: Anchor, target: (f64, f64)) -> Self {
        let Some(bbox) = self.bbox else {
            return self.clone();
        };
        let point = anchor.point(bbox);
        self.translated(target.0 - point.0, target.1 - point.1)
    }

    /// Konvexe Hülle aller tatsächlich geplanten Arbeitspunkte (Monotone Chain).
    /// Dient geräteneutral für konturfolgende Rahmenfahrten (Gummiband).
    pub fn convex_hull(&self) -> Vec<Pt> {
        let mut pts = Vec::new();
        for layer in &self.layers {
            match &layer.work {
                LayerWork::Cut { paths } => {
                    for path in paths {
                        pts.extend(path.points.iter().copied());
                    }
                }
                LayerWork::Fill { segments } => {
                    for s in segments {
                        pts.extend([(s.x0, s.y), (s.x1, s.y)]);
                    }
                }
                LayerWork::Raster { rows, .. } => {
                    for row in rows {
                        for &(x0, x1) in &row.runs {
                            pts.extend([(x0, row.y), (x1, row.y)]);
                        }
                    }
                }
            }
        }
        pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        pts.dedup();
        if pts.len() <= 2 {
            return pts;
        }
        fn cross(o: Pt, a: Pt, b: Pt) -> f64 {
            (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
        }
        let mut lower = Vec::new();
        for &p in &pts {
            while lower.len() >= 2
                && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0
            {
                lower.pop();
            }
            lower.push(p);
        }
        let mut upper = Vec::new();
        for &p in pts.iter().rev() {
            while upper.len() >= 2
                && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0
            {
                upper.pop();
            }
            upper.push(p);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    }

    /// Baut den Plan aus Shapes und Layern. Nur **aktive, nicht gesperrte**
    /// Layer kommen hinein; unsichtbare werden übersprungen. Rotation wird auf
    /// die Punkte angewandt, sodass Treiber nur noch fertige mm-Pfade sehen.
    ///
    /// Cut-Layer erhalten Kontur-Pfade, Fill-Layer eine Scanline-Füllung
    /// (Zeilenabstand aus `line_step_mm`). Bild-Layer werden **nicht** hier
    /// gerastert, weil `from_shapes` die Asset-Pixel nicht hat — dafür
    /// [`from_shapes_with_assets`](JobPlan::from_shapes_with_assets).
    pub fn from_shapes(shapes: &[Shape], layers: &[Layer]) -> JobPlan {
        // Ohne Asset-Auflösung liefert der Resolver nie Pixel — Bild-Layer
        // werden dann übersprungen (Treiber-Tests u. Ä. ohne Store).
        JobPlan::from_shapes_with_assets(shapes, layers, |_| None)
    }

    /// Wie [`from_shapes`](JobPlan::from_shapes), löst aber Bild-Assets auf:
    /// `resolve` liefert zu einer Asset-ID die **Graustufen-Pixel** (row-major
    /// `u8`) samt Pixelmaßen `(pixels, px_w, px_h)`, oder `None`, wenn das Asset
    /// fehlt. Image-Layer werden damit zu `LayerWork::Raster` (Schwellwert,
    /// raster.rs); der Core selbst fasst die Platte nicht an (Aufrufer liest den
    /// Store).
    pub fn from_shapes_with_assets<'a, F>(
        shapes: &[Shape],
        layers: &[Layer],
        mut resolve: F,
    ) -> JobPlan
    where
        F: FnMut(
            &str,
        ) -> Option<(
            std::borrow::Cow<'a, [u8]>,
            std::borrow::Cow<'a, [u8]>,
            usize,
            usize,
        )>,
    {
        let mut job_layers: Vec<JobLayer> = Vec::new();

        for (li, layer) in layers.iter().enumerate() {
            // Nur aktivierte Layer werden gebrannt. `visible` steuert nur die
            // Canvas-Anzeige, nicht den Job (ADR/Model: enabled ≠ visible).
            if !layer.enabled {
                continue;
            }

            // Bild-Layer: Shapes werden gerastert (Schwellwert), nicht als
            // Kontur/Fill behandelt. Nur die Bild-Shapes des Layers zählen.
            if layer.mode == LayerMode::Image {
                let (rows, texture) = raster_image_layer(shapes, li, layer, &mut resolve);
                if rows.is_empty() {
                    continue;
                }
                job_layers.push(JobLayer {
                    layer_id: li,
                    color: layer.color,
                    speed_mm_s: layer.speed_mm_s,
                    power_pct: layer.power_pct,
                    min_power_pct: layer.min_power_pct,
                    passes: layer.passes,
                    air_assist: layer.air_assist,
                    bidirectional: layer.bidirectional,
                    work: LayerWork::Raster { rows, texture },
                });
                continue;
            }

            let paths: Vec<(Option<u32>, Path)> = shapes
                .iter()
                .filter(|s| s.layer_id == li && (layer.mode.is_filled() || !s.fill_only))
                // `group_id` ist der Fallback für Projekte vor Einführung
                // der expliziten Füllpfad-ID.
                .map(|shape| (shape.fill_group_id.or(shape.group_id), shape_to_path(shape)))
                .collect();
            if paths.is_empty() {
                continue;
            }

            let work = if layer.mode.is_filled() {
                // Even/Odd gilt je zusammengesetztem Pfad; getrennte Pfade
                // werden danach vereinigt. Shapes ohne ID sind Einzelpfade.
                let mut grouped: Vec<(Option<u32>, Vec<&Path>)> = Vec::new();
                for (fill_group_id, path) in &paths {
                    if let Some(id) = fill_group_id {
                        if let Some((_, compound)) = grouped
                            .iter_mut()
                            .find(|(candidate, _)| *candidate == Some(*id))
                        {
                            compound.push(path);
                            continue;
                        }
                    }
                    grouped.push((*fill_group_id, vec![path]));
                }
                let contours: Vec<Vec<Contour>> = grouped
                    .iter()
                    .map(|(_, paths)| {
                        paths
                            .iter()
                            .map(|path| Contour {
                                points: &path.points,
                                closed: path.closed,
                            })
                            .collect()
                    })
                    .collect();
                let compounds: Vec<&[Contour]> = contours.iter().map(Vec::as_slice).collect();
                let segments = fill_compound_segments(&compounds, layer.line_step_mm);
                LayerWork::Fill { segments }
            } else {
                LayerWork::Cut {
                    paths: paths.into_iter().map(|(_, path)| path).collect(),
                }
            };

            job_layers.push(JobLayer {
                layer_id: li,
                color: layer.color,
                speed_mm_s: layer.speed_mm_s,
                power_pct: layer.power_pct,
                min_power_pct: layer.min_power_pct,
                passes: layer.passes,
                air_assist: layer.air_assist,
                bidirectional: layer.bidirectional,
                work,
            });
        }

        let bbox = bounding_box(&job_layers);
        JobPlan {
            layers: job_layers,
            bbox,
        }
    }

    /// Ob der Plan überhaupt Arbeit enthält.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// Rastert alle Bild-Shapes des Layers `li` zu An/Aus-Zeilen (Schwellwert).
/// `resolve` liefert die Graustufen-Pixel je Asset-ID. Gedrehte Bilder werden
/// achsenparallel gerastert und die Runs anschließend um das Box-Zentrum
/// rotiert (analog `shape_to_path`).
fn raster_image_layer<'a, F>(
    shapes: &[Shape],
    li: usize,
    layer: &Layer,
    resolve: &mut F,
) -> (Vec<RasterRow>, Option<RasterTexture>)
where
    F: FnMut(
        &str,
    ) -> Option<(
        std::borrow::Cow<'a, [u8]>,
        std::borrow::Cow<'a, [u8]>,
        usize,
        usize,
    )>,
{
    let mut out: Vec<RasterRow> = Vec::new();
    // Vorschau-Textur: beim Regelfall (ein Bild je Layer) die des Bildes. Bei
    // mehreren Bildern auf einem Layer trägt die Textur nur das erste (Randfall).
    let mut texture: Option<RasterTexture> = None;
    for s in shapes.iter().filter(|s| s.layer_id == li) {
        let Geo::Image {
            asset,
            x,
            y,
            w,
            h,
            params,
        } = &s.geo
        else {
            continue;
        };
        let Some((pixels, alpha, px_w, px_h)) = resolve(asset) else {
            continue;
        };
        // Achsenparallel rastern. Bild-Rotation wird hier bewusst NICHT
        // angewandt: horizontale An/Aus-Runs (y, x0, x1) können keine schräge
        // Strecke tragen, und ein „schiefes" Raster ist für Ausmalbilder ein
        // Randfall. Offen für später, falls gedrehte Bilder gebraucht werden
        // (dann müssten Runs echte 2D-Strecken sein).
        let src = RasterImage {
            pixels: &pixels,
            alpha: Some(&alpha),
            px_w,
            px_h,
        };
        let place = Placement {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
            step_mm: layer.line_step_mm,
        };
        out.extend(raster_rows(src, place, params, params.invert_laser));
        if texture.is_none() {
            texture = raster_texture(src, place, params, params.invert_laser);
        }
    }
    (out, texture)
}

/// Wandelt eine Shape (inkl. Rotation) in einen mm-Pfad. Die Kontur kommt aus
/// `Geo::outline_points` (eine Quelle mit den Geometrie-Ops, geo_ops.rs).
fn shape_to_path(s: &Shape) -> Path {
    let (mut points, closed) = s.geo.outline_points();
    if s.rotation != 0.0 {
        let (cx, cy) = s.bbox().center();
        for p in points.iter_mut() {
            *p = rotate_point(p.0, p.1, cx, cy, s.rotation);
        }
    }
    Path { points, closed }
}

fn bounding_box(layers: &[JobLayer]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut any = false;
    let mut acc = |x: f64, y: f64| {
        any = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for jl in layers {
        match &jl.work {
            LayerWork::Cut { paths } => {
                for p in paths {
                    for &(x, y) in &p.points {
                        acc(x, y);
                    }
                }
            }
            LayerWork::Fill { segments } => {
                for s in segments {
                    acc(s.x0, s.y);
                    acc(s.x1, s.y);
                }
            }
            LayerWork::Raster { rows, .. } => {
                for r in rows {
                    for &(x0, x1) in &r.runs {
                        acc(x0, r.y);
                        acc(x1, r.y);
                    }
                }
            }
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

/// Startmodus eines Jobs — „Starten von" (ADR 0006). Geräteneutral: jeder
/// Treiber setzt ihn in seine Form um (Ruida: Preamble-Byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartMode {
    /// Maschinen-absolute Koordinaten.
    #[default]
    Absolut,
    /// Relativ zur aktuellen Kopfposition.
    AktuellePosition,
    /// Relativ zum am Panel gesetzten Benutzerursprung.
    Benutzerursprung,
}

/// Startreferenz eines Jobs (ADR 0020): typisierte Auswahl unter „Starten von".
/// Ersetzt das flache [`StartMode`] überall dort, wo eine Referenz mit stabiler
/// ID gebraucht wird. Der Anzeigename ist nie die Referenz.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum StartReference {
    /// Maschinen-absolute Koordinaten (Ursprung = Maschinen-Null 0/0).
    #[default]
    Absolut,
    /// Relativ zur live gelesenen Kopfposition.
    AktuellePosition,
    /// Relativ zum am Ruida-Hardwarepanel gesetzten Benutzerursprung.
    Benutzerursprung,
    /// App-seitig gespeicherter Werkstück-Nullpunkt ([`crate::SavedOrigin`]-ID).
    GespeicherterNullpunkt { id: String },
}

impl StartReference {
    /// Controllerseitiger Startmodus dieser Referenz. Ein gespeicherter
    /// Nullpunkt wird app-seitig absolut aufgelöst (ADR 0020 §G) und ist
    /// deshalb für den Treiber ein absoluter Job.
    pub fn start_mode(&self) -> StartMode {
        match self {
            StartReference::Absolut | StartReference::GespeicherterNullpunkt { .. } => {
                StartMode::Absolut
            }
            StartReference::AktuellePosition => StartMode::AktuellePosition,
            StartReference::Benutzerursprung => StartMode::Benutzerursprung,
        }
    }

    /// ID des referenzierten gespeicherten Nullpunkts, falls vorhanden.
    pub fn saved_origin_id(&self) -> Option<&str> {
        match self {
            StartReference::GespeicherterNullpunkt { id } => Some(id),
            _ => None,
        }
    }
}

/// Job-Nullpunkt-Anker (3×3-Raster). Welcher Punkt der Zeichnung auf dem
/// Bezugspunkt landet — nur relevant, wenn `StartMode` nicht `Absolut` ist.
/// Reihenfolge = Frontend-Index 0..8 (0 = oben-links, 4 = Mitte, 8 = unten-rechts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    NW,
    N,
    NE,
    W,
    #[default]
    Center,
    E,
    SW,
    S,
    SE,
}

impl Anchor {
    /// Aus dem 3×3-Index (0..8) des Frontends. Andere Werte → Mitte.
    pub fn from_index(i: usize) -> Self {
        [
            Anchor::NW,
            Anchor::N,
            Anchor::NE,
            Anchor::W,
            Anchor::Center,
            Anchor::E,
            Anchor::SW,
            Anchor::S,
            Anchor::SE,
        ]
        .get(i)
        .copied()
        .unwrap_or(Anchor::Center)
    }

    /// Ankerpunkt (x, y) in mm innerhalb der Bounding-Box (min_x, min_y, max_x, max_y).
    pub fn point(self, bbox: (f64, f64, f64, f64)) -> (f64, f64) {
        let (minx, miny, maxx, maxy) = bbox;
        let (cx, cy) = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
        let x = match self {
            Anchor::NW | Anchor::W | Anchor::SW => minx,
            Anchor::N | Anchor::Center | Anchor::S => cx,
            Anchor::NE | Anchor::E | Anchor::SE => maxx,
        };
        let y = match self {
            Anchor::NW | Anchor::N | Anchor::NE => miny,
            Anchor::W | Anchor::Center | Anchor::E => cy,
            Anchor::SW | Anchor::S | Anchor::SE => maxy,
        };
        (x, y)
    }
}

/// Geräteneutrale Job-Parameter, die das Panel setzt (ADR 0006/0007). Kein
/// Gerätedetail — jeder Treiber setzt sie in seine Form um.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JobParams {
    pub start_mode: StartMode,
    pub anchor: Anchor,
}

/// Eine steuerbare Maschinenachse (geräteneutral). X/Y bilden die Ebene, Z ist
/// Fokus/Betthöhe, U die Rotary/Drehachse. Der Treiber bildet sie auf sein
/// Protokoll ab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineAxis {
    X,
    Y,
    Z,
    U,
}

/// Richtung einer Achsenbewegung (geräteneutral). Die *fachliche* Richtung; ob
/// „vorwärts" mechanisch links/rechts/hoch/runter bedeutet und ob ein Treiber
/// intern invertieren muss, ist Sache des Treibers (ADR 0021).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisDir {
    Forward,
    Backward,
}

/// Art der Jog-Auslösung. Es gibt fachlich EINE Achsenbewegung mit EINER
/// Richtung; nur das Auslösen kennt zwei Arten (ADR 0021 §B): antippen fährt
/// einen festen Schritt, halten fährt bis zum Stopp. `HoldStart`/`HoldStop`
/// rahmen einen gehaltenen Lauf; der Aufrufer (Watchdog) stellt das Stoppen
/// sicher.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JogMotion {
    /// Fester Schritt in mm (Betrag; die Richtung liefert `AxisDir`).
    Step(f64),
    HoldStart,
    HoldStop,
}

/// Momentaufnahme des Maschinenzustands (geräteneutral, mm).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MachineStatus {
    pub is_running: bool,
    pub is_paused: bool,
    pub pos_x_mm: f64,
    pub pos_y_mm: f64,
    /// Z-/U-Achsenposition (mm), sofern der Treiber sie liest. `None` = nicht
    /// gelesen. (Ein Wert sagt nichts über *Vorhandensein* der Achse — das ist
    /// eine Profil-Einstellung, ADR 0021 §A.)
    pub pos_z_mm: Option<f64>,
    pub pos_u_mm: Option<f64>,
    /// Rotary läuft klassisch über die Y-Achse (`rotary_enable` im Controller).
    /// Das IST ein echter Gerätezustand (im Gegensatz zur Z/U-Verfügbarkeit).
    pub rotary_on_y: bool,
}

/// Fehler der Live-Steuerung — geräteneutral gehalten (der Treiber wandelt
/// seine internen Fehler, z. B. UDP-Timeouts, hierher).
#[derive(Debug, Clone, PartialEq)]
pub enum DriverError {
    /// Keine Verbindung zur Maschine.
    NotConnected,
    /// Transport-/Kommunikationsfehler (Text vom Treiber).
    Transport(String),
    /// Der Treiber unterstützt diese Aktion nicht (z. B. GRBL-Stub).
    NotSupported,
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::NotConnected => write!(f, "Nicht mit einer Maschine verbunden."),
            DriverError::Transport(e) => write!(f, "Kommunikationsfehler: {e}"),
            DriverError::NotSupported => write!(f, "Dieser Treiber unterstützt die Aktion nicht."),
        }
    }
}

/// Was ein Treiber können muss. Der Core kennt keine Gerätedetails; die GUI
/// spricht ausschließlich über diesen Trait (ADR 0001/0006).
///
/// `name`/`compile` sind Pflicht (geräteunabhängiger Plan → gerätespezifische
/// Bytes). Die **Live-Steuerung** (connect/jog/…) hat Default-Implementierungen,
/// die `NotSupported`/`NotConnected` liefern — so bleibt ein Treiber baubar, der
/// (noch) keine Verbindung kann (z. B. GRBL), und überschreibt nur, was er
/// beherrscht.
pub trait MachineDriver {
    /// Name des Treibers (z. B. "Ruida", "GRBL").
    fn name(&self) -> &str;

    /// Optionale, geräteunabhängig beschriebene Treiberfähigkeiten.
    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities::default()
    }

    /// Maschinenparameter lesen, sofern der Treiber diese Capability anbietet.
    fn read_machine_settings(&self) -> Result<Vec<MachineSetting>, DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Maschinenparameter als rohe, treiberspezifische Registerwerte schreiben.
    fn write_machine_settings(&self, _changes: &[(u16, i64)]) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Übersetzt den Plan in gerätespezifische Job-Daten (Standardparameter:
    /// Absolut/Mitte). Für „Starten von"/Anker siehe [`compile_with`].
    ///
    /// [`compile_with`]: MachineDriver::compile_with
    fn compile(&self, plan: &JobPlan, layers: &[Layer]) -> Result<Vec<u8>, String> {
        self.compile_with(plan, layers, &JobParams::default())
    }

    /// Wie [`compile`](MachineDriver::compile), aber mit geräteneutralen
    /// Job-Parametern (Startmodus, Anker). Der Treiber setzt sie in seine Form
    /// um; der Plan selbst bleibt unverändert.
    fn compile_with(
        &self,
        plan: &JobPlan,
        layers: &[Layer],
        params: &JobParams,
    ) -> Result<Vec<u8>, String>;

    /// Dieselbe geordnete Bewegungsspur, aus der der Treiber seinen Job baut.
    fn execution_trace(
        &self,
        plan: &JobPlan,
        layers: &[Layer],
        params: &JobParams,
    ) -> Result<crate::ExecutionTrace, String>;

    /// Verbindung zur Maschine aufbauen (IP/Port bzw. serieller Anschluss).
    fn connect(&mut self, _target: &str) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Verbindung trennen.
    fn disconnect(&mut self) {}

    /// Aktuellen Maschinenstatus abfragen.
    fn status(&self) -> Result<MachineStatus, DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Kopf **relativ** um (dx, dy) mm mit `speed` bewegen (X/Y-Ebenen-Jog,
    /// Tippen — kann diagonal, liest die Position).
    fn jog(&self, _dx_mm: f64, _dy_mm: f64, _speed_mm_s: f64) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Einachsiges Jog einer Achse in eine Richtung (ADR 0021 §B). `motion`
    /// wählt Tippen (`Step`) oder gehaltenen Dauerlauf (`HoldStart`/`HoldStop`).
    /// Die *fachliche* Richtung `dir` gilt für beide Auslöse-Arten identisch —
    /// eine etwaige Protokoll-Inversion (Schritt- vs. Dauerlauf-Kommando) löst
    /// der Treiber intern auf, nicht der Aufrufer. Nur Treiber mit der Achse
    /// unterstützen das.
    fn jog_axis(
        &self,
        _axis: MachineAxis,
        _dir: AxisDir,
        _motion: JogMotion,
        _speed_mm_s: f64,
    ) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Referenzfahrt (absolut zum Nullpunkt).
    fn home(&self, _speed_mm_s: f64) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Kopf **absolut** und laserfrei nach (x, y) mm bewegen (Eilgang). Für
    /// „Anfahren" gespeicherter Nullpunkte (ADR 0020 §F); Grenz- und
    /// Zustandsprüfungen macht die Application vor dem Aufruf.
    fn move_to(&self, _x_mm: f64, _y_mm: f64, _speed_mm_s: f64) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Zum am Gerät gesetzten Benutzerursprung fahren (nicht die Maschinen-Null).
    fn go_origin(&self, _speed_mm_s: f64) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Den am Gerät gesetzten Benutzerursprung (mm) lesen, falls verfügbar.
    fn read_origin(&self) -> Result<(f64, f64), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Die Job-Bounding-Box abfahren, um die Platzierung zu prüfen. `params`
    /// bestimmt wie beim Job selbst Startmodus und Anker — der Rahmen fährt
    /// dort, wo der Job brennen würde.
    fn frame(
        &self,
        _plan: &JobPlan,
        _speed_mm_s: f64,
        _params: &JobParams,
    ) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Konvexe Außenkontur des Jobs abfahren (Gummiband-Rahmen), Platzierung
    /// wie [`frame`](MachineDriver::frame).
    fn rubber_frame(
        &self,
        _plan: &JobPlan,
        _speed_mm_s: f64,
        _params: &JobParams,
    ) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Fertig kompilierte Job-Bytes an die Maschine senden.
    fn send_job(&self, _bytes: &[u8]) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Sofort-Stopp.
    fn stop(&self) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Laufenden Job pausieren/fortsetzen.
    fn pause(&self) -> Result<(), DriverError> {
        Err(DriverError::NotSupported)
    }

    /// Welche Job-Aktionen dieser Treiber im Panel anbietet (ADR 0007). Das Panel
    /// rendert genau diese — kein fixer G-Code-/Sende-Knopf. Default: keine.
    fn actions(&self) -> Vec<crate::laser::JobAction> {
        Vec::new()
    }

    /// Eine gemeldete Aktion ausführen. Der Treiber entscheidet, was intern
    /// passiert (Ruida: kompilieren + UDP-Upload; GRBL: G-Code streamen). Gibt bei
    /// Bedarf einen Text fürs Frontend zurück (z. B. „Job gesendet (N Byte)").
    fn run_action(
        &self,
        _action: crate::laser::JobAction,
        _plan: &JobPlan,
        _layers: &[Layer],
        _params: &JobParams,
    ) -> Result<String, DriverError> {
        Err(DriverError::NotSupported)
    }
}

/// Fähigkeiten, die nicht jeder Maschinentreiber bereitstellt. Die UI zeigt
/// für fehlende Fähigkeiten „nicht unterstützt" statt erfundener Werte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriverCapabilities {
    pub machine_settings: bool,
    /// Kopfposition/Status live lesbar (`status()`).
    pub position_read: bool,
    /// Controllerseitiger Benutzerursprung lesbar (`read_origin()`).
    pub user_origin_read: bool,
    /// Absolute laserfreie Bewegung (`move_to()`).
    pub absolute_move: bool,
}

/// Einheit eines editierbaren Maschinenparameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineSettingUnit {
    Raw,
    Mm,
    MmPerSec,
    MmPerSec2,
    Percent,
    PermillePercent,
    StepLength,
    Pulse,
    Enum,
}

impl MachineSettingUnit {
    pub fn factor(self) -> f64 {
        match self {
            Self::Mm | Self::MmPerSec | Self::MmPerSec2 | Self::Pulse => 1_000.0,
            Self::PermillePercent => 10.0,
            Self::StepLength => 1_000_000.0,
            _ => 1.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::MmPerSec => "mm/s",
            Self::MmPerSec2 => "mm/s²",
            Self::Percent | Self::PermillePercent => "%",
            Self::StepLength => "µm",
            _ => "",
        }
    }
}

/// Ein vom Treiber gelieferter Maschinenparameter. Adressen und Rohwerte
/// bleiben bewusst opak; ihre Bedeutung kennt ausschließlich der Treiber.
#[derive(Debug, Clone)]
pub struct MachineSetting {
    pub address: u16,
    pub key: String,
    pub label: String,
    pub group: String,
    pub unit: MachineSettingUnit,
    pub curated: bool,
    pub writable: bool,
    pub bit_mask: Option<i64>,
    pub options: Vec<(i64, String)>,
    pub raw: Option<i64>,
    pub mirror: Option<u16>,
}

impl MachineSetting {
    pub fn value(&self) -> Option<f64> {
        self.raw.map(|value| value as f64 / self.unit.factor())
    }
}

/// Ob ein Layer-Modus (perspektivisch) Flächenfüllung braucht.
/// Für die spätere Fill/Raster-Erweiterung.
pub fn needs_fill(mode: LayerMode) -> bool {
    mode.is_filled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn state_one_rect() -> AppState {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        });
        s
    }

    #[test]
    fn rect_wird_geschlossener_pfad_mit_vier_punkten() {
        let s = state_one_rect();
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert_eq!(plan.layers.len(), 1);
        let LayerWork::Cut { paths } = &plan.layers[0].work else {
            panic!("Cut erwartet")
        };
        assert_eq!(paths.len(), 1);
        assert!(paths[0].closed);
        assert_eq!(paths[0].points.len(), 4);
        assert_eq!(paths[0].points[0], (10.0, 20.0));
    }

    #[test]
    fn fill_hilfskontur_wird_gefuellt_aber_nicht_geschnitten() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 4.0,
            y: 4.0,
            w: 2.0,
            h: 2.0,
        });
        let mut boundary = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
        );
        boundary.fill_only = true;
        boundary.fill_group_id = Some(1);
        s.shapes[0].fill_group_id = Some(1);
        s.shapes.insert(0, boundary);

        let cut = JobPlan::from_shapes(&s.shapes, &s.layers);
        let LayerWork::Cut { paths } = &cut.layers[0].work else {
            panic!("Cut erwartet")
        };
        assert_eq!(paths.len(), 1, "Hilfskontur darf nicht geschnitten werden");
        assert_eq!(cut.bbox, Some((4.0, 4.0, 6.0, 6.0)));

        s.layers[0].mode = LayerMode::Fill;
        s.layers[0].line_step_mm = 1.0;
        let fill = JobPlan::from_shapes(&s.shapes, &s.layers);
        let LayerWork::Fill { segments } = &fill.layers[0].work else {
            panic!("Fill erwartet")
        };
        assert!(segments
            .iter()
            .any(|line| line.x0 == 0.0 && line.x1 == 10.0));
        assert!(
            !segments
                .iter()
                .any(|line| line.y == 5.0 && line.x0 < 5.0 && line.x1 > 5.0),
            "innere Musterkontur muss als Loch ausgespart bleiben"
        );
    }

    #[test]
    fn svg_import_bis_jobfill_erhaelt_loch_und_vereinigt_getrennten_pfad() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="-10" y="-10" width="120" height="120" fill="white"/>
            <path fill="black" fill-rule="evenodd"
                  d="M 0 0 H 100 V 100 H 0 Z M 10 10 H 90 V 90 H 10"/>
            <path fill="black" d="M 45 20 H 55 V 80 H 45"/>
        </svg>"#;
        let compounds = crate::import::import_vector_compounds(svg, "svg").unwrap();
        assert_eq!(compounds.len(), 2);
        let mut state = AppState::new();
        state.add_compound_polylines(compounds);
        state.layers[0].mode = LayerMode::Fill;
        state.layers[0].line_step_mm = 0.1;

        let plan = JobPlan::from_shapes(&state.shapes, &state.layers);
        let LayerWork::Fill { segments } = &plan.layers[0].work else {
            panic!("Fill erwartet")
        };
        let mm = 25.4 / 96.0;
        let y = segments
            .iter()
            .min_by(|a, b| (a.y - 50.0 * mm).abs().total_cmp(&(b.y - 50.0 * mm).abs()))
            .unwrap()
            .y;
        let filled = |x: f64| {
            segments
                .iter()
                .any(|segment| segment.y == y && segment.x0 <= x * mm && x * mm <= segment.x1)
        };
        assert!(filled(5.0), "Außenring bleibt gefüllt");
        assert!(
            !filled(30.0),
            "implizit geschlossene Innenkontur bleibt Loch"
        );
        assert!(
            filled(50.0),
            "getrennter Schaftpfad wird mit dem Ring vereinigt"
        );
    }

    #[test]
    fn bbox_umschliesst_geometrie() {
        let s = state_one_rect();
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert_eq!(plan.bbox, Some((10.0, 20.0, 40.0, 60.0)));
    }

    #[test]
    fn plan_wird_in_rechts_unten_nullpunkt_transformiert() {
        let mut state = AppState::new();
        state.add_shape(Geo::Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        });
        let plan = JobPlan::from_shapes(&state.shapes, &state.layers)
            .transformed_for_bed(crate::BedOrigin::BottomRight, (600.0, 400.0));
        assert_eq!(plan.bbox, Some((560.0, 340.0, 590.0, 380.0)));
    }

    #[test]
    fn konvexe_huelle_umschliesst_motiv() {
        let mut st = AppState::new();
        st.add_shape(Geo::Rect {
            x: 2.0,
            y: 3.0,
            w: 8.0,
            h: 5.0,
        });
        let hull = JobPlan::from_shapes(&st.shapes, &st.layers).convex_hull();
        assert_eq!(hull.len(), 4);
        assert!(hull.contains(&(2.0, 3.0)));
        assert!(hull.contains(&(10.0, 3.0)));
        assert!(hull.contains(&(10.0, 8.0)));
        assert!(hull.contains(&(2.0, 8.0)));
    }

    #[test]
    fn deaktivierter_layer_wird_uebersprungen() {
        let mut s = state_one_rect();
        s.layers[0].enabled = false;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert!(plan.is_empty());
    }

    #[test]
    fn unsichtbarer_aber_aktivierter_layer_wird_gebrannt() {
        // visible steuert nur die Anzeige, nicht den Job.
        let mut s = state_one_rect();
        s.layers[0].visible = false;
        s.layers[0].enabled = true;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert!(!plan.is_empty(), "unsichtbarer aktivierter Layer brennt");
    }

    #[test]
    fn rotation_wird_auf_punkte_angewandt() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        });
        s.shapes[0].rotation = 90.0;
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        let LayerWork::Cut { paths } = &plan.layers[0].work else {
            panic!("Cut erwartet")
        };
        // Um 90° gedreht muss die Bounding-Box der Punkte ~20 breit, ~100 hoch sein.
        let xs: Vec<f64> = paths[0].points.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = paths[0].points.iter().map(|p| p.1).collect();
        let w = xs.iter().cloned().fold(f64::MIN, f64::max)
            - xs.iter().cloned().fold(f64::MAX, f64::min);
        let h = ys.iter().cloned().fold(f64::MIN, f64::max)
            - ys.iter().cloned().fold(f64::MAX, f64::min);
        assert!((w - 20.0).abs() < 1e-6, "Breite nach Drehung ~20, war {w}");
        assert!((h - 100.0).abs() < 1e-6, "Höhe nach Drehung ~100, war {h}");
    }

    #[test]
    fn image_layer_wird_gerastert_wenn_asset_aufloesbar() {
        // Bild-Layer mit 2×2-Asset (obere Zeile schwarz, untere weiß), 2×2 mm.
        let mut s = AppState::new();
        s.add_image("cafe".into(), 0.0, 0.0, 2.0, 2.0);
        s.layers.last_mut().unwrap().line_step_mm = 1.0;

        // Resolver liefert die Pixel des Assets „cafe": Zeile 0 schwarz, Zeile 1 weiß.
        let pixels = vec![0u8, 0, 255, 255];
        let plan = JobPlan::from_shapes_with_assets(&s.shapes, &s.layers, |id| {
            (id == "cafe").then_some((
                std::borrow::Cow::Borrowed(pixels.as_slice()),
                std::borrow::Cow::Owned(vec![255; 4]),
                2,
                2,
            ))
        });

        assert_eq!(plan.layers.len(), 1);
        let LayerWork::Raster { rows, .. } = &plan.layers[0].work else {
            panic!("Raster erwartet, war {:?}", plan.layers[0].work)
        };
        // Nur die schwarze Zeile (oben) erzeugt einen Run über die volle Breite.
        assert_eq!(rows.len(), 1, "nur die schwarze Zeile brennt");
        assert_eq!(rows[0].runs.len(), 1);
        assert!((rows[0].runs[0].0 - 0.0).abs() < 1e-6);
        assert!((rows[0].runs[0].1 - 2.0).abs() < 1e-6);
    }

    #[test]
    fn bild_import_bis_raster_plan_end_to_end() {
        use crate::assets::{import_image, load_asset_luma};

        // Echtes 4×4-PNG (linke Hälfte schwarz, rechte weiß) importieren …
        let dir = std::env::temp_dir().join(format!("studio_job_raster_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut img = image::GrayImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.put_pixel(x, y, image::Luma([if x < 2 { 0 } else { 255 }]));
            }
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let meta = import_image(&dir, &png, "kante.png").unwrap();

        // … als Bild-Layer platzieren (8×8 mm, step 2 mm ⇒ 4×4 Zielraster) …
        let mut s = AppState::new();
        s.add_image(meta.id.clone(), 0.0, 0.0, 8.0, 8.0);
        s.layers.last_mut().unwrap().line_step_mm = 2.0;

        // … und über die Store-Auflösung zu einem Raster-Plan bauen.
        let plan = JobPlan::from_shapes_with_assets(&s.shapes, &s.layers, |id| {
            let (px, w, h) = load_asset_luma(&dir, &id.to_string()).ok()?;
            Some((
                std::borrow::Cow::Owned(px),
                std::borrow::Cow::Owned(vec![255; (w * h) as usize]),
                w as usize,
                h as usize,
            ))
        });

        let LayerWork::Raster { rows, .. } = &plan.layers[0].work else {
            panic!("Raster erwartet")
        };
        // Vier Zeilen, jede mit einem Run über die linke (schwarze) Hälfte 0..4 mm.
        assert_eq!(rows.len(), 4);
        for r in rows {
            assert_eq!(r.runs.len(), 1, "je Zeile ein Run");
            assert!((r.runs[0].0 - 0.0).abs() < 1e-6);
            assert!(
                (r.runs[0].1 - 4.0).abs() < 0.5,
                "Run endet ~Bildmitte (4mm)"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_layer_ohne_asset_wird_uebersprungen() {
        // Ohne Asset-Auflösung (from_shapes) rastert nichts → leerer Plan.
        let mut s = AppState::new();
        s.add_image("fehlt".into(), 0.0, 0.0, 2.0, 2.0);
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        assert!(plan.is_empty(), "unauflösbares Bild = kein Raster-Layer");
    }

    #[test]
    fn plan_verschiebung_legt_anker_auf_zielkoordinate() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 50.0,
            h: 30.0,
        });
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        // NW-Anker der BBox (10,10) soll auf (100,50) landen.
        let placed = plan.placed_with_anchor_at(Anchor::NW, (100.0, 50.0));
        assert_eq!(placed.bbox, Some((100.0, 50.0, 150.0, 80.0)));
        // Mitte-Anker: BBox-Zentrum (35,25) auf (100,50).
        let centered = plan.placed_with_anchor_at(Anchor::Center, (100.0, 50.0));
        assert_eq!(centered.bbox, Some((75.0, 35.0, 125.0, 65.0)));
        // Leerer Plan bleibt unverändert.
        let empty = JobPlan {
            layers: Vec::new(),
            bbox: None,
        }
        .placed_with_anchor_at(Anchor::NW, (5.0, 5.0));
        assert_eq!(empty.bbox, None);
    }

    #[test]
    fn startreferenz_kennt_controllermodus_und_id() {
        assert_eq!(StartReference::Absolut.start_mode(), StartMode::Absolut);
        assert_eq!(
            StartReference::AktuellePosition.start_mode(),
            StartMode::AktuellePosition
        );
        assert_eq!(
            StartReference::Benutzerursprung.start_mode(),
            StartMode::Benutzerursprung
        );
        let saved = StartReference::GespeicherterNullpunkt { id: "o-1".into() };
        // App-seitig aufgelöst → für den Treiber absolut.
        assert_eq!(saved.start_mode(), StartMode::Absolut);
        assert_eq!(saved.saved_origin_id(), Some("o-1"));
        assert_eq!(StartReference::Absolut.saved_origin_id(), None);
        // Persistenz-Roundtrip (lokale Bedienpräferenz, ADR 0020 §E).
        let json = serde_json::to_string(&saved).unwrap();
        assert_eq!(
            serde_json::from_str::<StartReference>(&json).unwrap(),
            saved
        );
    }

    #[test]
    fn ellipse_wird_polygonisiert() {
        let mut s = AppState::new();
        s.add_shape(Geo::Ellipse {
            cx: 0.0,
            cy: 0.0,
            rx: 10.0,
            ry: 5.0,
        });
        let plan = JobPlan::from_shapes(&s.shapes, &s.layers);
        let LayerWork::Cut { paths } = &plan.layers[0].work else {
            panic!("Cut erwartet")
        };
        assert!(paths[0].closed);
        assert!(paths[0].points.len() >= 32);
    }
}
