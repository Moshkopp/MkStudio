//! Die Tauri-Commands, nach Verantwortlichkeit gruppiert. Jede Datei ist eine
//! zusammenhängende Aufgabe (Laser, Formen, Projekt …). Die Commands ziehen den
//! geteilten Zustand aus `crate::shared`.

pub mod laser;
pub mod project;
pub mod edit;
pub mod image;
pub mod shapes;
