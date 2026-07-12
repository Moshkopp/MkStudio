//! Canvas-Darstellung: aus dem Editor-/Interaktionszustand Vertices bauen.
//!
//! Zwei getrennte Puffer (ADR 0010, Render-Revision):
//! - [`scene::base_vertices`] ist der gecachte Basis-Puffer (Tisch-Gitter,
//!   Füllung, Konturen). Er hängt nur an der Geometrie und wird über die
//!   Render-Revision invalidiert.
//! - [`overlay::overlay_vertices`] ist das jeden Frame neu gebaute, winzige
//!   Overlay (Auswahl, Handles, Live-Zeichenvorschau) — kamera-abhängig.
//!
//! Beide sind reine Funktionen ohne GPU-/Eingabe-Bezug; sie lesen nur Zustand.

pub mod gestures;
pub mod input;
pub mod overlay;
pub mod scene;
pub mod state;

pub use state::CanvasState;
