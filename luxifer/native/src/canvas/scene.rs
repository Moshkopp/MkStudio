//! Gecachter Basis-Vertexpuffer: Tisch-Gitter, Füllung und Konturen. Hängt nur
//! an der Geometrie (über die Render-Revision invalidiert), nicht an der Auswahl
//! — die Auswahl-Akzentuierung liegt bewusst im Overlay.

use luxifer_application::EditorSession;

use crate::scene_geo::{self, Vertex};

pub struct BaseGeometry {
    pub vertices: Vec<Vertex>,
    pub fill_vertices: Vec<Vertex>,
    pub fill_batches: Vec<scene_geo::FillBatch>,
    /// Ende des Bett-/Gitter-Bereichs im gemeinsamen Vertexpuffer.
    pub background_end: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BaseBuildTimings {
    pub fill_ms: f64,
    pub lines_ms: f64,
}

/// Baut die gecachten Zeichendaten (Tisch-Fläche, Shapes-Füllung/Kontur).
/// Das Gitter ist kamera-abhängig und liegt im eigenen Grid-Puffer
/// (`scene_geo::viewport_grid`), nicht in diesem Cache.
/// Liefert zusätzlich getrennte CPU-Zeiten für die zwei szenengroßen
/// Aufbereitungsschritte. Der Renderer nutzt diese Werte nur für das opt-in
/// Performance-Protokoll; die Geometrie bleibt identisch.
pub fn base_vertices_profiled(
    session: &EditorSession,
    origin: luxifer_core::BedOrigin,
) -> (BaseGeometry, BaseBuildTimings) {
    let mut v = scene_geo::bed_base(session.bed_w_mm as f32, session.bed_h_mm as f32, origin);
    let background_end = v.len() as u32;
    let fill_started = std::time::Instant::now();
    let (fill_vertices, fill_batches) = scene_geo::solid_fills(session);
    let fill_ms = fill_started.elapsed().as_secs_f64() * 1_000.0;
    let lines_started = std::time::Instant::now();
    v.extend(scene_geo::shape_lines(session));
    let lines_ms = lines_started.elapsed().as_secs_f64() * 1_000.0;
    // Der laufende Punkt-Zug (Polyline/Spline/Bézier/Polygon) wird im Overlay
    // gezeichnet (jeden Frame, damit das Gummiband der Maus folgt).
    (
        BaseGeometry {
            vertices: v,
            fill_vertices,
            fill_batches,
            background_end,
        },
        BaseBuildTimings { fill_ms, lines_ms },
    )
}

/// Material-Vorlage der Laser-Vorschau: Untergrund- und Brennfarbe. Die
/// Vorschau zeigt das Werkstück, nicht den Messtisch — auf Schiefer graviert
/// der Laser hell, auf Holz dunkel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewMaterial {
    HolzHell,
    HolzDunkel,
    #[default]
    Schiefer,
}

impl PreviewMaterial {
    pub const ALL: [PreviewMaterial; 3] = [
        PreviewMaterial::HolzHell,
        PreviewMaterial::HolzDunkel,
        PreviewMaterial::Schiefer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PreviewMaterial::HolzHell => "Holz hell",
            PreviewMaterial::HolzDunkel => "Holz dunkel",
            PreviewMaterial::Schiefer => "Schiefer",
        }
    }

    /// Untergrund (Werkstück-Fläche).
    pub fn bed(self) -> [f32; 4] {
        match self {
            PreviewMaterial::HolzHell => [0.82, 0.68, 0.47, 1.0],
            PreviewMaterial::HolzDunkel => [0.38, 0.26, 0.16, 1.0],
            PreviewMaterial::Schiefer => [0.10, 0.11, 0.12, 1.0],
        }
    }

    /// Brennfarbe der Arbeitswege (Gravur/Schnitt auf dem Material).
    pub fn burn(self) -> [f32; 4] {
        match self {
            PreviewMaterial::HolzHell => [0.24, 0.13, 0.05, 1.0],
            PreviewMaterial::HolzDunkel => [0.08, 0.04, 0.02, 1.0],
            PreviewMaterial::Schiefer => [0.94, 0.94, 0.94, 1.0],
        }
    }

    /// Leerfahrten: dezent und kontrastarm zum jeweiligen Untergrund.
    pub fn travel(self) -> [f32; 4] {
        match self {
            PreviewMaterial::HolzHell => [0.35, 0.30, 0.40, 0.45],
            PreviewMaterial::HolzDunkel => [0.75, 0.70, 0.65, 0.35],
            PreviewMaterial::Schiefer => [0.55, 0.60, 0.68, 0.45],
        }
    }
}

/// sRGB → linear. Die Materialfarben sind als sRGB-Wunschoptik definiert
/// (so nutzt sie auch das egui-Panel direkt); der Canvas schreibt aber in
/// einen sRGB-Framebuffer, der lineare Werte beim Speichern enkodiert —
/// ohne diese Umkehrung erschiene Schiefer mittelgrau statt fast schwarz.
pub fn srgb_to_linear(c: [f32; 4]) -> [f32; 4] {
    let f = |x: f32| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

/// Kennzahlen der Jobvorschau für die Legende. Wird beim Vertex-Aufbau
/// nebenbei gefüllt (kein zweiter Preview-Lauf).
#[derive(Default)]
pub struct PreviewLegend {
    /// Material, mit dem gebaut wurde (für die Farbfelder der Legende).
    pub material: PreviewMaterial,
    pub has_travel: bool,
    /// Ob es überhaupt Arbeitsinhalte gibt (Wege oder Rasterbilder).
    pub has_content: bool,
    /// Arbeitsweg (Laser an) in mm.
    pub work_len_mm: f64,
    /// Leerfahrten in mm.
    pub travel_len_mm: f64,
    pub scan_offset_active: bool,
    /// Bounding-Box der Job-Geometrie (mm).
    pub bbox: Option<(f64, f64, f64, f64)>,
}

/// Vollständiger Preview-Aufbau: Vertices für die Bewegungen plus die
/// Raster-Runs der Bild-Layer und die Legende.
pub struct PreviewGeometry {
    pub vertices: Vec<Vertex>,
    pub background_end: u32,
    /// Verarbeitete Bild-Rasterungen (Pixel 255 = gebrannt) an ihrer mm-Box.
    pub rasters: Vec<luxifer_core::RasterTexture>,
    pub legend: PreviewLegend,
}

/// Read-only Jobpfad auf der Material-Bühne: Arbeitsbewegungen in Brennfarbe,
/// Leerfahrten dezent, Bild-Layer als ihre tatsächlich gefahrenen Scanlinien. Grundlage ist
/// ausschließlich die Application-Preview (derselbe JobPlan wie Export/Treiber).
pub fn preview_vertices(
    session: &EditorSession,
    preview: &luxifer_core::ExecutionTrace,
    material: PreviewMaterial,
    show_travel: bool,
    show_laser_path: bool,
    show_scan_offset: bool,
    bed_origin: luxifer_core::BedOrigin,
) -> PreviewGeometry {
    let mut v = scene_geo::bed_material(
        session.bed_w_mm as f32,
        session.bed_h_mm as f32,
        srgb_to_linear(material.bed()),
    );
    let background_end = v.len() as u32;
    // Die Trace liegt absichtlich in Maschinenkoordinaten vor. Der Canvas ist
    // dagegen immer oben-links orientiert. BedOrigin::transform ist eine
    // Spiegelung und damit ihre eigene Umkehrfunktion; so bleibt die Anzeige
    // aufrecht, ohne die an den Laser gesendete Bewegung zu verändern.
    let display_point = |point: luxifer_core::Pt| {
        bed_origin.transform(point.0, point.1, (session.bed_w_mm, session.bed_h_mm))
    };
    let bbox = preview
        .moves
        .iter()
        .flat_map(|movement| [display_point(movement.from), display_point(movement.to)])
        .fold(None, |bbox: Option<(f64, f64, f64, f64)>, point| {
            Some(match bbox {
                None => (point.0, point.1, point.0, point.1),
                Some((x0, y0, x1, y1)) => (
                    x0.min(point.0),
                    y0.min(point.1),
                    x1.max(point.0),
                    y1.max(point.1),
                ),
            })
        });
    let mut legend = PreviewLegend {
        material,
        scan_offset_active: preview.scan_offset_active,
        bbox,
        ..Default::default()
    };
    let burn = srgb_to_linear(material.burn());
    let travel = srgb_to_linear(material.travel());
    for movement in &preview.moves {
        let color = match movement.kind {
            luxifer_core::ExecutionKind::Travel => {
                legend.has_travel = true;
                legend.travel_len_mm += movement.len_mm();
                // Bei vielen Objekten übertünchen die Leerfahrten das Motiv —
                // sie zählen für die Kennzahlen, werden aber nur auf Wunsch
                // gezeichnet.
                if !show_travel {
                    continue;
                }
                travel
            }
            _ => {
                legend.has_content = true;
                legend.work_len_mm += movement.len_mm();
                if show_laser_path {
                    [0.05, 0.9, 0.25, 0.5]
                } else {
                    burn
                }
            }
        };
        let (from, to) = if show_scan_offset {
            (movement.from, movement.to)
        } else {
            (movement.ideal_from, movement.ideal_to)
        };
        let from = display_point(from);
        let to = display_point(to);
        scene_geo::push_seg(
            &mut v,
            [from.0 as f32, from.1 as f32],
            [to.0 as f32, to.1 as f32],
            color,
        );
    }
    PreviewGeometry {
        vertices: v,
        background_end,
        rasters: Vec::new(),
        legend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_spiegelt_maschinenkoordinaten_zurueck_in_den_canvas() {
        let mut session = EditorSession::default();
        session.bed_w_mm = 200.0;
        session.bed_h_mm = 100.0;
        let mut builder = luxifer_core::TraceBuilder::new(false);
        builder.set_head((190.0, 90.0));
        builder.work(
            (190.0, 90.0),
            (180.0, 80.0),
            (190.0, 90.0),
            (180.0, 80.0),
            luxifer_core::ExecutionKind::Cut,
            0,
        );

        let geometry = preview_vertices(
            &session,
            &builder.finish(),
            PreviewMaterial::Schiefer,
            false,
            false,
            false,
            luxifer_core::BedOrigin::BottomRight,
        );
        let path = &geometry.vertices[geometry.background_end as usize..];
        assert_eq!(path.len(), 6, "eine Linie wird als zwei Dreiecke erzeugt");
        assert_eq!(geometry.legend.bbox, Some((10.0, 10.0, 20.0, 20.0)));
    }

    #[test]
    fn solid_fill_folgt_dem_verschieben() {
        // Reproduktion Nutzerbefund: „Objekt mit Fill verschieben — Füllung
        // bleibt stehen". Die Szene muss nach der Move-Geste neue Vertices
        // liefern, deren Fill-Bereich mitgewandert ist.
        let mut session = EditorSession::default();
        {
            let state = session.state_mut_for_migration();
            state.add_shape(luxifer_core::Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            });
            state.layers[0].mode = luxifer_core::LayerMode::Fill;
        }
        session.selected = vec![0];
        let rev_before = session.render_rev();
        let before = base_vertices_profiled(&session, Default::default()).0;

        session.begin_edit();
        session.translate_edit(50.0, 0.0);
        session.commit_edit();

        // Die Render-Revision MUSS sich geändert haben, sonst baut der
        // Renderer den gecachten Puffer nie neu.
        assert_ne!(session.render_rev(), rev_before);

        let after = base_vertices_profiled(&session, Default::default()).0;
        // Die selektierte Kontur liegt im separaten Auswahlbuffer; die feste
        // Flächenfüllung im Basisbuffer muss dennoch mitwandern.
        assert!(after
            .fill_vertices
            .iter()
            .all(|vertex| vertex.pos[0] >= 45.0));
        // Und vorher lagen sie links.
        assert!(before
            .fill_vertices
            .iter()
            .all(|vertex| vertex.pos[0] <= 15.0));
    }

    #[test]
    fn solid_fill_folgt_der_echten_move_geste_frame_fuer_frame() {
        // Stellt den echten App-Ablauf nach: Klick auf ein Fill-Objekt,
        // Cursor-Moves, pro „Frame" der Renderer-Vergleich über render_rev.
        use crate::camera::Camera;
        use crate::canvas::CanvasState;

        let mut session = EditorSession::default();
        {
            let state = session.state_mut_for_migration();
            state.add_shape(luxifer_core::Geo::Rect {
                x: 10.0,
                y: 10.0,
                w: 20.0,
                h: 20.0,
            });
            state.layers[0].mode = luxifer_core::LayerMode::Fill;
            state.selected.clear();
        }
        let mut cam = Camera::new();
        cam.viewport = [800.0, 600.0];
        cam.center = [0.0, 0.0];
        cam.scale = 1.0;
        let mut canvas = CanvasState::new(cam);
        canvas.tool = crate::tools::Tool::Select;

        // Vektoren werden an der Kontur gegriffen: linke Kante bei (10,20).
        let press = canvas.cam.world_to_screen([10.0, 20.0]);
        canvas.cursor = press;
        let last_rev = session.render_rev();
        canvas.on_mouse(&mut session, winit::event::MouseButton::Left, true);

        // Drei Cursor-Moves à +10 Weltpixel: Core/Fill-Cache bleiben während
        // der GPU-Vorschau unverändert, nur der Uniform-Offset steigt.
        for step in 1..=3 {
            let target = canvas
                .cam
                .world_to_screen([10.0 + step as f64 * 10.0, 20.0]);
            canvas.on_cursor_move(&mut session, target);
            let rev = session.render_rev();
            assert_eq!(
                rev, last_rev,
                "Frame {step}: kein Core-Rebuild im Live-Move"
            );
            assert_eq!(canvas.live_move_offset(), [step as f32 * 10.0, 0.0]);
            let g = base_vertices_profiled(&session, Default::default()).0;
            let cached_fill_min_x = g
                .fill_vertices
                .iter()
                .map(|vertex| vertex.pos[0])
                .fold(f32::MAX, f32::min);
            assert!((cached_fill_min_x - 10.0).abs() < 1.0);
        }
        canvas.on_mouse(&mut session, winit::event::MouseButton::Left, false);
        assert_ne!(session.render_rev(), last_rev);
        let committed = base_vertices_profiled(&session, Default::default()).0;
        let fill_min_x = committed
            .fill_vertices
            .iter()
            .map(|vertex| vertex.pos[0])
            .fold(f32::MAX, f32::min);
        assert!((fill_min_x - 40.0).abs() < 1.0);
    }

    #[test]
    fn bett_und_szenengeometrie_haben_getrennte_renderbereiche() {
        let mut session = EditorSession::default();
        session
            .state_mut_for_migration()
            .add_image("asset".into(), 0.0, 0.0, 20.0, 10.0);
        session.selected.clear();

        let geometry = base_vertices_profiled(&session, Default::default()).0;

        assert!(geometry.background_end > 0);
        assert!((geometry.background_end as usize) < geometry.vertices.len());
    }
}
