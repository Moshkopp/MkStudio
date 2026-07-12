//! Typisierte UI-Absichten: Ein Panel zeichnet und liefert `UiAction`s zurück,
//! statt den `App`-Zustand direkt zu mutieren. Der Root führt sie über
//! `App::dispatch` aus. So bleibt die UI von der Anwendungslogik entkoppelt
//! (ADR 0011: „UI erzeugt Absicht, App koordiniert").
//!
//! Das Enum wächst schnittweise: Es deckt zunächst nur die Aktionen der bereits
//! migrierten Panels ab. Panels, die noch `&mut App` erhalten, tragen hier noch
//! nichts bei.

use luxifer_core::{Align, Distribute};

/// Eine vom UI ausgelöste Absicht. Rein beschreibend — kein Verhalten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiAction {
    /// Auswahl ausrichten.
    Align(Align),
    /// Auswahl verteilen/Abstände angleichen.
    Distribute(Distribute),
    /// Auswahl gruppieren.
    Group,
    /// Gruppierung der Auswahl lösen.
    Ungroup,
    /// Auswahl mit festem Abstand packen (mm).
    Nest(f64),
    /// Bett mit der Auswahl füllen (Abstand mm).
    NestFill(f64),
}
