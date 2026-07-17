//! Werkzeugleiste (links): 2-spaltiges Icon-Grid, 5 Gruppen wie die
//! Tauri-ToolsPanel. Enthält den geteilten `icon_button`-Helfer.

use super::action::UiAction;
use super::ICON_BUTTON_SIDE;
use crate::tools::Tool;

/// Quadratischer Icon-Button (Werkzeugleiste). `on` = aktiv (Akzent),
/// `dim` = Stub/deaktiviert dezenter. Gibt true bei Klick zurück.
pub(super) fn icon_button(
    ui: &mut egui::Ui,
    side: f32,
    icon: &str,
    tip: &str,
    on: bool,
    dim: bool,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let accent = ui.visuals().selection.stroke.color;
    let bg = if on {
        accent.gamma_multiply(0.72)
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().faint_bg_color
    };
    ui.painter().rect(
        rect,
        7.0,
        bg,
        egui::Stroke::new(
            if on { 1.5 } else { 1.0 },
            if on {
                accent
            } else {
                ui.visuals().window_stroke.color
            },
        ),
        egui::StrokeKind::Inside,
    );
    let fg = if dim {
        ui.visuals().weak_text_color()
    } else {
        ui.visuals().text_color()
    };
    // Icon-Box zentriert (etwas kleiner als der Button).
    let pad = side * 0.22;
    let ic = egui::Rect::from_min_max(
        rect.min + egui::vec2(pad, pad),
        rect.max - egui::vec2(pad, pad),
    );
    crate::icons::draw(ui.painter(), ic, icon, fg);
    resp.on_hover_text(tip).clicked()
}

/// Werkzeuge in einem 2-Spalten-Grid; gibt das geklickte Werkzeug zurück.
fn tool_grid(ui: &mut egui::Ui, side: f32, gap: f32, cur: Tool, tools: &[Tool]) -> Option<Tool> {
    let mut clicked = None;
    egui::Grid::new(("tg", tools.first().map(|t| t.label()).unwrap_or("")))
        .spacing([gap, gap])
        .show(ui, |ui| {
            for (i, &t) in tools.iter().enumerate() {
                if icon_button(ui, side, t.icon(), t.label(), cur == t, false) {
                    clicked = Some(t);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    clicked
}

/// 2-spaltige Werkzeugleiste, 5 Gruppen wie die Tauri-ToolsPanel — nur Icons.
/// `cur` = aktives Werkzeug (nur für die Markierung). Gibt die Absichten zurück.
pub(super) fn tools_panel(ui: &mut egui::Ui, cur: Tool, selection: usize) -> Vec<UiAction> {
    use crate::tools::ToolAction as A;
    let mut actions = Vec::new();
    ui.add_space(4.0);
    let gap = 4.0;
    let side = ICON_BUTTON_SIDE;

    group_label(ui, "AUSWAHL");
    // Gruppe 1: Auswahl. Gleiche Kantenlänge wie alle anderen Werkzeuge;
    // die frühere volle Panelbreite machte diesen Knopf unverhältnismäßig groß.
    ui.horizontal(|ui| {
        if icon_button(
            ui,
            side,
            "select",
            "Auswahl / Verschieben",
            cur == Tool::Select,
            false,
        ) {
            actions.push(UiAction::SelectTool(Tool::Select));
        }
    });
    divider(ui);
    // Gruppe 2: Zeichnen & Formen.
    group_label(ui, "ZEICHNEN");
    if let Some(t) = tool_grid(
        ui,
        side,
        gap,
        cur,
        &[
            Tool::Rect,
            Tool::Ellipse,
            Tool::Polygon,
            Tool::Line,
            Tool::Polyline,
            Tool::Spline,
            Tool::Bezier,
        ],
    ) {
        actions.push(UiAction::SelectTool(t));
    }
    // Text (Sofort-Aktion) + Node (Werkzeug) in derselben Gruppe.
    egui::Grid::new("tg_textnode")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(
                ui,
                side,
                "text",
                "Text einfügen (Text zu Pfad)",
                false,
                false,
            ) {
                actions.push(UiAction::OpenTextDialog);
            }
            if icon_button(
                ui,
                side,
                "node",
                "Knoten bearbeiten",
                cur == Tool::Node,
                false,
            ) {
                actions.push(UiAction::SelectTool(Tool::Node));
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 3: Operationen.
    group_label(ui, "BEARBEITEN");
    egui::Grid::new("tg_ops")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(
                ui,
                side,
                "trim",
                "Abschnitt zwischen Schnittpunkten trimmen",
                cur == Tool::Trim,
                false,
            ) {
                actions.push(UiAction::SelectTool(Tool::Trim));
            }
            if icon_button(
                ui,
                side,
                "bridge",
                "Haltesteg (Linie über die Kontur ziehen)",
                cur == Tool::Bridge,
                false,
            ) {
                actions.push(UiAction::SelectTool(Tool::Bridge));
            }
            ui.end_row();
            if icon_button(ui, side, "boolean", "Boolean (Auswahl)", false, false) {
                actions.push(UiAction::ToolAction(A::Boolean));
            }
            if icon_button(
                ui,
                side,
                "fillet",
                "Ecken verrunden (Auswahl)",
                false,
                false,
            ) {
                actions.push(UiAction::ToolAction(A::Fillet));
            }
            ui.end_row();
            if icon_button(
                ui,
                side,
                "pattern-fill",
                "Muster füllen (Auswahl)",
                false,
                false,
            ) {
                actions.push(UiAction::ToolAction(A::PatternFill));
            }
            if icon_button(
                ui,
                side,
                "offset",
                "Offset / parallele Kontur (Auswahl)",
                false,
                false,
            ) {
                actions.push(UiAction::ToolAction(A::Offset));
            }
            ui.end_row();
            if icon_button(
                ui,
                side,
                "measure",
                "Messen (Klick+Ziehen)",
                cur == Tool::Measure,
                false,
            ) {
                actions.push(UiAction::SelectTool(Tool::Measure));
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 4: Spiegeln.
    group_label(ui, "ANORDNEN");
    egui::Grid::new("tg_mirror")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(ui, side, "mirror-h", "Horizontal spiegeln", false, false) {
                actions.push(UiAction::MirrorH);
            }
            if icon_button(ui, side, "mirror-v", "Vertikal spiegeln", false, false) {
                actions.push(UiAction::MirrorV);
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 5: Nesting. Geometrische Platzierungsaktionen gehören zu den
    // Werkzeugen; der zweite Header bleibt für die aktuelle Auswahlgröße frei.
    egui::Grid::new("tg_nesting")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if selection >= 2
                && icon_button(ui, side, "nest", "Auswahl packen (2 mm)", false, false)
            {
                actions.push(UiAction::Nest(2.0));
            } else if selection < 2 {
                icon_button(
                    ui,
                    side,
                    "nest",
                    "Mindestens zwei Objekte auswählen",
                    false,
                    true,
                );
            }
            if selection >= 1
                && icon_button(ui, side, "nest-fill", "Bett füllen (2 mm)", false, false)
            {
                actions.push(UiAction::NestFill(2.0));
            } else if selection < 1 {
                icon_button(ui, side, "nest-fill", "Objekt auswählen", false, true);
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 6: Untersetzer.
    egui::Grid::new("tg_coaster")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(
                ui,
                side,
                "coaster-rect",
                "4×2 eckige Untersetzer",
                false,
                false,
            ) {
                actions.push(UiAction::InsertCoasters(false));
            }
            if icon_button(
                ui,
                side,
                "coaster-circle",
                "4×2 runde Untersetzer",
                false,
                false,
            ) {
                actions.push(UiAction::InsertCoasters(true));
            }
            ui.end_row();
        });
    actions
}

/// Dünner horizontaler Trenner zwischen Werkzeuggruppen.
fn divider(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let w = ui.available_width() * 0.8;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    let y = rect.center().y;
    let x0 = rect.center().x - w / 2.0;
    ui.painter().line_segment(
        [egui::pos2(x0, y), egui::pos2(x0 + w, y)],
        egui::Stroke::new(1.0, ui.visuals().window_stroke.color.gamma_multiply(0.7)),
    );
    ui.add_space(3.0);
}

fn group_label(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(9.0)
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(2.0);
}
