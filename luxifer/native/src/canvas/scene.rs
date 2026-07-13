//! Gecachter Basis-Vertexpuffer: Tisch-Gitter, Füllung und Konturen. Hängt nur
//! an der Geometrie (über die Render-Revision invalidiert), nicht an der Auswahl
//! — die Auswahl-Akzentuierung liegt bewusst im Overlay.

use luxifer_application::EditorSession;

use crate::scene_geo::{self, Vertex};

pub struct BaseGeometry {
    pub vertices: Vec<Vertex>,
    /// Ende des Bett-/Gitter-Bereichs im gemeinsamen Vertexpuffer.
    pub background_end: u32,
}

/// Baut die gecachten Zeichendaten (Tisch-Fläche, Shapes-Füllung/Kontur).
/// Das Gitter ist kamera-abhängig und liegt im eigenen Grid-Puffer
/// (`scene_geo::viewport_grid`), nicht in diesem Cache.
pub fn base_vertices(session: &EditorSession) -> BaseGeometry {
    let mut v = scene_geo::bed_base(session.bed_w_mm as f32, session.bed_h_mm as f32);
    let background_end = v.len() as u32;
    // Füllung zuerst (liegt unter den Konturen), dann die Umrisse.
    v.extend(scene_geo::fill_lines(session));
    v.extend(scene_geo::shape_lines(session));
    // Der laufende Punkt-Zug (Polyline/Spline/Bézier/Polygon) wird im Overlay
    // gezeichnet (jeden Frame, damit das Gummiband der Maus folgt).
    BaseGeometry {
        vertices: v,
        background_end,
    }
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
    /// Bounding-Box der Job-Geometrie (mm).
    pub bbox: Option<(f64, f64, f64, f64)>,
}

/// Vollständiger Preview-Aufbau: Vertices für die Bewegungen plus die
/// verarbeiteten Rastertexturen der Bild-Layer und die Legende.
pub struct PreviewGeometry {
    pub vertices: Vec<Vertex>,
    pub background_end: u32,
    /// Verarbeitete Bild-Rasterungen (Pixel 255 = gebrannt) an ihrer mm-Box.
    pub rasters: Vec<luxifer_core::RasterTexture>,
    pub legend: PreviewLegend,
}

/// Read-only Jobpfad auf der Material-Bühne: Arbeitsbewegungen in Brennfarbe,
/// Leerfahrten dezent, Bild-Layer als verarbeitete Rastertextur. Grundlage ist
/// ausschließlich die Application-Preview (derselbe JobPlan wie Export/Treiber).
pub fn preview_vertices(
    session: &EditorSession,
    selection_only: bool,
    material: PreviewMaterial,
    show_travel: bool,
) -> PreviewGeometry {
    let mut v = scene_geo::bed_material(
        session.bed_w_mm as f32,
        session.bed_h_mm as f32,
        srgb_to_linear(material.bed()),
    );
    let background_end = v.len() as u32;
    let preview = session.job_preview(selection_only);
    let mut legend = PreviewLegend {
        material,
        bbox: preview.bbox,
        has_content: !preview.rasters.is_empty(),
        ..Default::default()
    };
    let burn = srgb_to_linear(material.burn());
    let travel = srgb_to_linear(material.travel());
    for movement in &preview.moves {
        let color = match movement.kind {
            luxifer_core::preview::MoveKind::Travel => {
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
                burn
            }
        };
        scene_geo::push_seg(
            &mut v,
            [movement.from.0 as f32, movement.from.1 as f32],
            [movement.to.0 as f32, movement.to.1 as f32],
            color,
        );
    }
    PreviewGeometry {
        vertices: v,
        background_end,
        rasters: preview.rasters,
        legend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_scanlines_folgen_dem_verschieben() {
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
        let before = base_vertices(&session);

        session.begin_edit();
        session.translate_edit(50.0, 0.0);
        session.commit_edit();

        // Die Render-Revision MUSS sich geändert haben, sonst baut der
        // Renderer den gecachten Puffer nie neu.
        assert_ne!(session.render_rev(), rev_before);

        let after = base_vertices(&session);
        // Alle Szenen-Vertices (nach dem Bett) liegen jetzt bei x >= 45 —
        // Kontur UND Scanlines sind mitgewandert.
        let scene_after = &after.vertices[after.background_end as usize..];
        assert!(!scene_after.is_empty());
        assert!(
            scene_after.iter().all(|v| v.pos[0] >= 45.0),
            "Füllung/Kontur muss der Verschiebung folgen"
        );
        // Und vorher lagen sie links.
        let scene_before = &before.vertices[before.background_end as usize..];
        assert!(scene_before.iter().all(|v| v.pos[0] <= 15.0));
    }

    #[test]
    fn fill_folgt_der_echten_move_geste_frame_fuer_frame() {
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

        // Klick mitten aufs Rechteck (Weltpunkt (20,20) → Screen).
        let press = canvas.cam.world_to_screen([20.0, 20.0]);
        canvas.cursor = press;
        let mut last_rev = session.render_rev();
        canvas.on_mouse(&mut session, winit::event::MouseButton::Left, true);

        // Drei Cursor-Moves à +10 Weltpixel; nach jedem prüft der „Renderer".
        for step in 1..=3 {
            let target = canvas
                .cam
                .world_to_screen([20.0 + step as f64 * 10.0, 20.0]);
            canvas.on_cursor_move(&mut session, target);
            let rev = session.render_rev();
            assert_ne!(rev, last_rev, "Frame {step}: render_rev muss steigen");
            last_rev = rev;
            let g = base_vertices(&session);
            let scene = &g.vertices[g.background_end as usize..];
            let min_x = scene.iter().map(|v| v.pos[0]).fold(f32::MAX, f32::min);
            assert!(
                (min_x - (10.0 + step as f32 * 10.0)).abs() < 1.0,
                "Frame {step}: Szene (inkl. Fill) muss bei x≈{} beginnen, ist {min_x}",
                10.0 + step as f32 * 10.0
            );
        }
        canvas.on_mouse(&mut session, winit::event::MouseButton::Left, false);
    }

    #[test]
    fn bett_und_szenengeometrie_haben_getrennte_renderbereiche() {
        let mut session = EditorSession::default();
        session
            .state_mut_for_migration()
            .add_image("asset".into(), 0.0, 0.0, 20.0, 10.0);

        let geometry = base_vertices(&session);

        assert!(geometry.background_end > 0);
        assert!((geometry.background_end as usize) < geometry.vertices.len());
    }
}
