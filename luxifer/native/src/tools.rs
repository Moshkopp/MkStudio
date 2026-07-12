//! Werkzeug-Zustand des Canvas. Reines UI-Anliegen (welches Tool ist aktiv,
//! welcher Zug läuft gerade) — die eigentliche Mutation macht immer der Core.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Rect,
    Ellipse,
    Polygon,
    Line,
    Polyline,
    Spline,
    Bezier,
    Measure,
    Node,
}

/// Haupt-Ansicht (Reiterleiste oben), analog zur Tauri-App.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Projekt,
    Design,
    Laser,
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            View::Projekt => "Projekt",
            View::Design => "Design",
            View::Laser => "Laser",
        }
    }
}

/// Laser-Bedien-Zustand (UI-seitig). Ohne echten Treiber-Anschluss im nativen
/// Umbau — die Aktionen loggen vorerst; die Treiber-Verdrahtung kommt später.
pub struct LaserUi {
    pub jog_step: f64,
    pub jog_speed: f64,
    /// Job-Nullpunkt-Anker (0..8, 4 = Mitte).
    pub anchor: usize,
    pub selection_only: bool,
    /// Startmodus des Jobs (Absolut / aktuelle Position / Benutzerursprung).
    pub start_mode: luxifer_core::StartMode,
}

impl Default for LaserUi {
    fn default() -> Self {
        Self {
            jog_step: 10.0,
            jog_speed: 100.0,
            anchor: 4,
            selection_only: false,
            start_mode: luxifer_core::StartMode::Absolut,
        }
    }
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Auswahl",
            Tool::Rect => "Rechteck",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygon",
            Tool::Line => "Linie",
            Tool::Polyline => "Polylinie",
            Tool::Spline => "Spline",
            Tool::Bezier => "Bézier",
            Tool::Measure => "Messen",
            Tool::Node => "Knoten",
        }
    }

    /// Icon-Name (siehe icons.rs).
    pub fn icon(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Rect => "rect",
            Tool::Ellipse => "ellipse",
            Tool::Polygon => "polygon",
            Tool::Line => "line",
            Tool::Polyline => "polyline",
            Tool::Spline => "spline",
            Tool::Bezier => "bezier",
            Tool::Measure => "measure",
            Tool::Node => "node",
        }
    }
}

/// Sofort-Befehl auf der Auswahl (kein Zeichenmodus). Entspricht den `action`-
/// Werkzeugen der Tauri-ToolsPanel + den Arrange-Aktionen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolAction {
    Boolean,
    Fillet,
    Offset,
    PatternFill,
    Bridge,
}

/// Typisierte Tastatur-Aktion des Canvas. Die Zuordnung Taste→Aktion ist reine
/// Native-Präsentation; die eigentliche Mutation macht die `EditorSession`.
/// Bewusst getrennt vom Auslösen, damit die Zuordnung ohne winit/egui testbar
/// ist (Fokusregeln inklusive).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shortcut {
    Save,
    SaveVersion,
    Delete,
    Cancel,
    FinishPolygon,
    Undo,
    Redo,
    SelectTool(Tool),
    /// Leertaste gedrückt/losgelassen (Pan-Modifier) — kein einmaliger Befehl.
    PanModifier(bool),
}

/// Gedrückte Taste, die für die Zuordnung relevant ist. Entkoppelt die reine
/// Shortcut-Logik von `winit::keyboard::KeyCode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    S,
    Delete,
    Escape,
    Enter,
    Space,
    V,
    R,
    E,
    P,
    Z,
    Y,
}

/// Modifier-Zustand zum Zeitpunkt des Tastendrucks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

/// Bildet eine gedrückte Taste auf eine typisierte Aktion ab.
///
/// `text_editing` ist true, wenn egui gerade Tastatureingaben will (ein
/// Textfeld oder Dialog hat den Fokus). Dann feuert **kein** Canvas-Shortcut,
/// damit Tippen hinter einem offenen Layer-/Text-Dialog die Szene nicht
/// mutiert. `pressed` unterscheidet Down/Up; nur die Leertaste braucht beide
/// Flanken, alle anderen Befehle lösen auf Down aus.
pub fn resolve_shortcut(
    key: Key,
    mods: Mods,
    pressed: bool,
    text_editing: bool,
) -> Option<Shortcut> {
    // Die Leertaste ist ein gehaltener Pan-Modifier, kein Befehl. Auch sie ruht,
    // solange ein Textfeld tippt (sonst scrollt Pan beim Leerzeichen).
    if key == Key::Space {
        return if text_editing {
            None
        } else {
            Some(Shortcut::PanModifier(pressed))
        };
    }
    if !pressed || text_editing {
        return None;
    }
    match key {
        Key::S if mods.ctrl && mods.shift => Some(Shortcut::SaveVersion),
        Key::S if mods.ctrl => Some(Shortcut::Save),
        Key::S => None,
        Key::Z if mods.ctrl => Some(Shortcut::Undo),
        Key::Y if mods.ctrl => Some(Shortcut::Redo),
        // Undo/Redo verlangen Strg — ein nacktes „z" ist kein Undo.
        Key::Z | Key::Y => None,
        Key::Delete => Some(Shortcut::Delete),
        Key::Escape => Some(Shortcut::Cancel),
        Key::Enter => Some(Shortcut::FinishPolygon),
        Key::V => Some(Shortcut::SelectTool(Tool::Select)),
        Key::R => Some(Shortcut::SelectTool(Tool::Rect)),
        Key::E => Some(Shortcut::SelectTool(Tool::Ellipse)),
        Key::P => Some(Shortcut::SelectTool(Tool::Polygon)),
        Key::Space => None,
    }
}

/// Laufende Maus-Geste im Canvas (zwischen Press und Release).
pub enum Drag {
    None,
    /// Canvas verschieben (mittlere Maustaste oder Leertaste+links).
    Pan,
    /// Auswahl-Rechteck aufziehen (Welt-Startpunkt).
    Marquee {
        start: [f64; 2],
    },
    /// Selektierte Shapes verschieben (letzter Welt-Punkt).
    MoveShapes {
        last: [f64; 2],
    },
    /// Neues Rechteck/Ellipse aufziehen (Welt-Startpunkt).
    DrawBox {
        start: [f64; 2],
    },
    /// Auswahl über ein Handle skalieren. `handle` = gezogene Ecke/Kante,
    /// `start_box` = Auswahl-BBox bei Drag-Beginn, `orig` = Snapshot der
    /// selektierten Shapes bei Drag-Beginn (Index + Shape). So wird bei jedem
    /// Maus-Schritt vom Ausgangszustand skaliert statt vom bereits skalierten
    /// (sonst schaukelt sich die Größe exponentiell auf).
    Resize {
        handle: luxifer_core::Handle,
        start_box: luxifer_core::BBox,
        orig: Vec<(usize, luxifer_core::Shape)>,
    },
    /// Auswahl drehen. `pivot` = Mittelpunkt, `orig` = Snapshot bei Drag-Beginn,
    /// `start_angle` = Mauswinkel bei Beginn. Rotation immer vom Ausgangszustand.
    Rotate {
        pivot: [f64; 2],
        start_angle: f64,
        orig: Vec<(usize, luxifer_core::Shape)>,
    },
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        shift: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        shift: true,
    };

    #[test]
    fn fokussiertes_textfeld_unterdrueckt_jeden_canvas_shortcut() {
        // Kern der Fehlerklasse: Tippen in einem Dialog darf die Szene nicht
        // mutieren und kein Werkzeug wechseln.
        for key in [Key::Delete, Key::Escape, Key::V, Key::R, Key::Enter] {
            assert_eq!(resolve_shortcut(key, NONE, true, true), None);
        }
        // Auch Strg+Z/Y und Strg+S ruhen, solange ein Textfeld tippt.
        assert_eq!(resolve_shortcut(Key::Z, CTRL, true, true), None);
        assert_eq!(resolve_shortcut(Key::S, CTRL, true, true), None);
    }

    #[test]
    fn undo_und_redo_verlangen_strg() {
        assert_eq!(resolve_shortcut(Key::Z, NONE, true, false), None);
        assert_eq!(resolve_shortcut(Key::Y, NONE, true, false), None);
        assert_eq!(
            resolve_shortcut(Key::Z, CTRL, true, false),
            Some(Shortcut::Undo)
        );
        assert_eq!(
            resolve_shortcut(Key::Y, CTRL, true, false),
            Some(Shortcut::Redo)
        );
    }

    #[test]
    fn speichern_unterscheidet_version_per_shift() {
        assert_eq!(
            resolve_shortcut(Key::S, CTRL, true, false),
            Some(Shortcut::Save)
        );
        assert_eq!(
            resolve_shortcut(Key::S, CTRL_SHIFT, true, false),
            Some(Shortcut::SaveVersion)
        );
        // Ohne Strg ist „s" kein Speicherbefehl (könnte ein Werkzeug werden).
        assert_eq!(resolve_shortcut(Key::S, NONE, true, false), None);
    }

    #[test]
    fn werkzeug_und_editierbefehle_nur_auf_tastendruck() {
        assert_eq!(
            resolve_shortcut(Key::V, NONE, true, false),
            Some(Shortcut::SelectTool(Tool::Select))
        );
        assert_eq!(
            resolve_shortcut(Key::Delete, NONE, true, false),
            Some(Shortcut::Delete)
        );
        // Loslassen löst keinen dieser Befehle aus.
        assert_eq!(resolve_shortcut(Key::V, NONE, false, false), None);
        assert_eq!(resolve_shortcut(Key::Delete, NONE, false, false), None);
    }

    #[test]
    fn leertaste_ist_pan_modifier_mit_beiden_flanken() {
        assert_eq!(
            resolve_shortcut(Key::Space, NONE, true, false),
            Some(Shortcut::PanModifier(true))
        );
        assert_eq!(
            resolve_shortcut(Key::Space, NONE, false, false),
            Some(Shortcut::PanModifier(false))
        );
        // Aber nicht, während ein Textfeld tippt.
        assert_eq!(resolve_shortcut(Key::Space, NONE, true, true), None);
    }
}
