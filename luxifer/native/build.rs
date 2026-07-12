//! Versionsstring zur BUILD-ZEIT aus git ableiten (gleiches Muster wie das
//! Tauri-Frontend): Major.Minor kommt aus dem letzten Tag (z. B. v0.9), der
//! hintere Teil ist die Commit-Zahl seit dem Tag plus kurzer Hash — steigt
//! automatisch mit jedem Commit, ohne manuelles Pflegen.
//!
//! `git describe --tags --always --dirty` liefert z. B.:
//!   v0.9-3-gd544270          (3 Commits seit Tag v0.9, Hash d544270)
//!   v0.9-3-gd544270-dirty    (zusätzlich uncommittete Änderungen)
//!   d544270                  (Fallback ohne Tag)

use std::process::Command;

fn main() {
    let version = git(&["describe", "--tags", "--always", "--dirty"])
        .unwrap_or_else(|| "unbekannt".to_string());
    println!("cargo:rustc-env=LUXIFER_VERSION={version}");

    // Kurzer Commit-Hash separat, für die About-Anzeige.
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "-".to_string());
    println!("cargo:rustc-env=LUXIFER_COMMIT={hash}");

    // Bei jedem Commit neu bauen, damit die Version aktuell bleibt. HEAD zeigt
    // je nach Branch-Zustand auf packed-refs oder eine lose Ref — beide watchen.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
