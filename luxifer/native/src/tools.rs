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
    /// Abschnitt einer Kontur zwischen den nächsten Schnittpunkten entfernen.
    Trim,
    /// Haltesteg: Linie über eine Kontur ziehen, Breite einstellen, bestätigen
    /// → die Kontur wird im Steg-Band aufgetrennt und quer wieder geschlossen.
    Bridge,
}

/// Haupt-Ansicht (Reiterleiste oben), analog zur Tauri-App.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Projekt,
    Design,
    Preview,
    Laser,
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            View::Projekt => "Projekt",
            View::Design => "Design",
            View::Preview => "Vorschau",
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
            Tool::Trim => "Trimmen",
            Tool::Bridge => "Haltesteg",
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
            Tool::Trim => "trim",
            Tool::Bridge => "bridge",
        }
    }
}

/// Sofort-Befehl auf der Auswahl (kein Zeichenmodus). Entspricht den `action`-
/// Werkzeugen der Tauri-ToolsPanel + den Arrange-Aktionen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolAction {
    Boolean,
    Fillet,
    Offset,
    PatternFill,
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
    /// Alle Objekte auf dem Canvas auswählen (Strg+A).
    SelectAll,
    /// Kamera einpassen: auf die Auswahl, sonst auf alle Objekte (F).
    FitView,
    Group,
    Ungroup,
    OpenText,
    SwitchView(View),
    OpenAssets,
    SelectTool(Tool),
    /// Leertaste gedrückt/losgelassen (Pan-Modifier) — kein einmaliger Befehl.
    PanModifier(bool),
}

/// Gedrückte Taste, die für die Zuordnung relevant ist. Entkoppelt die reine
/// Shortcut-Logik von `winit::keyboard::Key` (logische Taste, Systemlayout).
pub use luxifer_core::ShortcutKey as Key;

/// Modifier-Zustand zum Zeitpunkt des Tastendrucks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Bildet eine gedrückte Taste auf eine typisierte Aktion ab.
///
/// `input_blocked` ist true, wenn ein Textfeld den Tastaturfokus hat **oder**
/// ein modaler Dialog offen ist. Dann feuert **kein** Canvas-Shortcut, damit
/// Tippen hinter einem offenen Layer-/Text-/Laser-Dialog die Szene nicht
/// mutiert. `pressed` unterscheidet Down/Up; nur die Leertaste braucht beide
/// Flanken, alle anderen Befehle lösen auf Down aus.
pub fn resolve_shortcut(
    key: Key,
    mods: Mods,
    pressed: bool,
    input_blocked: bool,
) -> Option<Shortcut> {
    resolve_shortcut_with_bindings(
        key,
        mods,
        pressed,
        input_blocked,
        &luxifer_core::ShortcutBindings::default(),
    )
}

pub fn resolve_shortcut_with_bindings(
    key: Key,
    mods: Mods,
    pressed: bool,
    input_blocked: bool,
    bindings: &luxifer_core::ShortcutBindings,
) -> Option<Shortcut> {
    // Die Leertaste ist ein gehaltener Pan-Modifier, kein Befehl. Das Loslassen
    // muss IMMER durch — sonst bliebe `space_down` hängen, wenn während
    // gehaltenem Space ein Dialog den Fokus übernimmt. Nur das Drücken (Pan an)
    // ruht bei blockierter Eingabe.
    if key == Key::Space {
        return if pressed && input_blocked {
            None
        } else {
            Some(Shortcut::PanModifier(pressed))
        };
    }
    if !pressed || input_blocked {
        return None;
    }
    match key {
        Key::Escape => Some(Shortcut::Cancel),
        Key::Enter => Some(Shortcut::FinishPolygon),
        Key::Space => None,
        _ => resolve_trigger(
            luxifer_core::ShortcutTrigger::Key(luxifer_core::ShortcutChord {
                key,
                ctrl: mods.ctrl,
                shift: mods.shift,
                alt: mods.alt,
            }),
            bindings,
        ),
    }
}

pub fn resolve_trigger(
    trigger: luxifer_core::ShortcutTrigger,
    bindings: &luxifer_core::ShortcutBindings,
) -> Option<Shortcut> {
    bindings.resolve(trigger).map(shortcut_for_action)
}

fn shortcut_for_action(action: luxifer_core::ShortcutAction) -> Shortcut {
    use luxifer_core::ShortcutAction as A;
    match action {
        A::Save => Shortcut::Save,
        A::SaveVersion => Shortcut::SaveVersion,
        A::Undo => Shortcut::Undo,
        A::Redo => Shortcut::Redo,
        A::SelectAll => Shortcut::SelectAll,
        A::Delete => Shortcut::Delete,
        A::Group => Shortcut::Group,
        A::Ungroup => Shortcut::Ungroup,
        A::FitView => Shortcut::FitView,
        A::ToolSelect => Shortcut::SelectTool(Tool::Select),
        A::ToolRect => Shortcut::SelectTool(Tool::Rect),
        A::ToolEllipse => Shortcut::SelectTool(Tool::Ellipse),
        A::ToolPolygon => Shortcut::SelectTool(Tool::Polygon),
        A::ToolLine => Shortcut::SelectTool(Tool::Line),
        A::ToolPolyline => Shortcut::SelectTool(Tool::Polyline),
        A::ToolSpline => Shortcut::SelectTool(Tool::Spline),
        A::ToolBezier => Shortcut::SelectTool(Tool::Bezier),
        A::ToolMeasure => Shortcut::SelectTool(Tool::Measure),
        A::ToolNode => Shortcut::SelectTool(Tool::Node),
        A::ToolTrim => Shortcut::SelectTool(Tool::Trim),
        A::ToolBridge => Shortcut::SelectTool(Tool::Bridge),
        A::OpenText => Shortcut::OpenText,
        A::ViewProject => Shortcut::SwitchView(View::Projekt),
        A::ViewDesign => Shortcut::SwitchView(View::Design),
        A::ViewLaser => Shortcut::SwitchView(View::Laser),
        A::ViewPreview => Shortcut::SwitchView(View::Preview),
        A::OpenAssets => Shortcut::OpenAssets,
    }
}

/// Laufende Maus-Geste im Canvas (zwischen Press und Release).
pub enum Drag {
    None,
    /// Canvas verschieben (mittlere Maustaste oder Leertaste+links).
    Pan,
    /// Trimmen bei gehaltener linker Maustaste. Der Abstand zum letzten
    /// Treffer verhindert ein sofortiges erneutes Trimmen desselben Restes.
    TrimStroke {
        last_trim: [f64; 2],
    },
    /// Auswahl-Rechteck aufziehen (Welt-Startpunkt).
    Marquee {
        start: [f64; 2],
    },
    /// Selektierte Shapes verschieben. `start` bleibt während eines GPU-Live-
    /// Moves unverändert; `last` ist die aktuelle Cursorposition.
    MoveShapes {
        start: [f64; 2],
        last: [f64; 2],
        gpu_live: bool,
    },
    /// Neues Rechteck/Ellipse aufziehen (Welt-Startpunkt).
    DrawBox {
        start: [f64; 2],
    },
    /// Tangente des zuletzt gesetzten Bézier-Ankers aufziehen.
    BezierHandle {
        node: usize,
    },
    /// Anker oder Tangenten-Endpunkt eines bestehenden Pfads verschieben.
    EditNode {
        shape: usize,
        node: usize,
        part: luxifer_core::bezier::NodePart,
    },
    /// Endpunkt der Haltesteg-Linie ziehen (0 = Start, 1 = Ende).
    BridgeEnd {
        end: usize,
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
        target_box: luxifer_core::BBox,
        gpu_live: bool,
    },
    /// Auswahl drehen. `pivot` = Mittelpunkt, `orig` = Snapshot bei Drag-Beginn,
    /// `start_angle` = Mauswinkel bei Beginn. Rotation immer vom Ausgangszustand.
    Rotate {
        pivot: [f64; 2],
        start_angle: f64,
        orig: Vec<(usize, luxifer_core::Shape)>,
        start_box: luxifer_core::BBox,
        delta_deg: f64,
        gpu_live: bool,
    },
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
        alt: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        shift: false,
        alt: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        shift: true,
        alt: false,
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
        assert_eq!(
            resolve_shortcut(Key::Z, CTRL_SHIFT, true, false),
            Some(Shortcut::Redo)
        );
    }

    #[test]
    fn alles_auswaehlen_verlangt_strg_fitview_verbietet_es() {
        assert_eq!(
            resolve_shortcut(Key::A, CTRL, true, false),
            Some(Shortcut::SelectAll)
        );
        assert_eq!(resolve_shortcut(Key::A, NONE, true, false), None);
        assert_eq!(
            resolve_shortcut(Key::F, NONE, true, false),
            Some(Shortcut::FitView)
        );
        // Strg+F bleibt frei (z. B. künftige Suche).
        assert_eq!(resolve_shortcut(Key::F, CTRL, true, false), None);
        // Und wie alle Canvas-Shortcuts: ruhen, solange ein Textfeld tippt.
        assert_eq!(resolve_shortcut(Key::A, CTRL, true, true), None);
        assert_eq!(resolve_shortcut(Key::F, NONE, true, true), None);
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
        // Pan an ruht bei blockierter Eingabe (Dialog/Textfeld).
        assert_eq!(resolve_shortcut(Key::Space, NONE, true, true), None);
    }

    #[test]
    fn leertaste_loslassen_kommt_auch_bei_blockade_durch() {
        // Sonst bliebe `space_down` hängen, wenn während gehaltenem Space ein
        // Dialog den Fokus übernimmt: das Key-up träfe auf `input_blocked`.
        assert_eq!(
            resolve_shortcut(Key::Space, NONE, false, true),
            Some(Shortcut::PanModifier(false))
        );
    }

    #[test]
    fn benutzerdefinierte_belegung_ersetzt_den_standard_lookup() {
        let mut bindings = luxifer_core::ShortcutBindings::default();
        let custom = luxifer_core::ShortcutTrigger::Key(luxifer_core::ShortcutChord::key(Key::Q));
        bindings
            .reassign(luxifer_core::ShortcutAction::ToolRect, custom)
            .unwrap();

        assert_eq!(
            resolve_shortcut_with_bindings(Key::Q, NONE, true, false, &bindings),
            Some(Shortcut::SelectTool(Tool::Rect))
        );
        assert_eq!(
            resolve_shortcut_with_bindings(Key::R, NONE, true, false, &bindings),
            Some(Shortcut::SelectTool(Tool::Rect))
        );
        assert_eq!(
            resolve_shortcut_with_bindings(Key::Q, NONE, true, true, &bindings),
            None
        );
    }
}
