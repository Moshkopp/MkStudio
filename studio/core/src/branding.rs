//! Einzige Quelle für austauschbare Produkt- und Komponentennamen.
//!
//! Technische Crate-, Verzeichnis- und Protokollnamen bleiben bewusst neutral.
//! Ein späteres Rebranding ändert deshalb nur die Werte in diesem Modul sowie
//! die zugehörigen Bilddateien, nicht die Architektur des Workspace.

/// Öffentlich sichtbarer Name der Anwendung und Produktsuite.
pub const PRODUCT_NAME: &str = env!("PRODUCT_NAME");

/// Öffentlich sichtbarer Name der Desktopanwendung.
pub const STUDIO_NAME: &str = PRODUCT_NAME;

/// Öffentlich sichtbarer Name des Koordinationsdienstes.
pub const HUB_NAME: &str = env!("HUB_NAME");

/// Stabiler Protokoll-Identifier. Er ist kein Branding und darf bei einem
/// Produkt-Rename nicht geändert werden.
pub const HUB_PROTOCOL_ID: &str = env!("HUB_PROTOCOL_ID");

/// Stabiler technischer Anwendungsbezeichner für Desktopintegration.
pub const APP_ID: &str = env!("APP_ID");

/// Stabiler technischer Name des lokalen Datenverzeichnisses.
pub const DATA_DIR_NAME: &str = env!("DATA_DIR_NAME");

/// Anzeigename für Projektdateien.
pub const PROJECT_FILE_LABEL: &str = concat!(env!("PRODUCT_NAME"), "-Projekt");
