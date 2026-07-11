use std::process::Command;

fn main() {
    // Versionsstring zur BUILD-ZEIT aus git ableiten (tag-relativ, ADR-Muster
    // wie viele Rust/Go-Projekte). Der Major.Minor kommt aus dem letzten Tag
    // (z. B. v0.8), der hintere Teil ist die Commit-Zahl seit dem Tag plus der
    // kurze Hash — steigt automatisch mit jedem Commit, ohne manuelles Pflegen.
    //
    // `git describe --tags --always --dirty` liefert z. B.:
    //   v0.8-12-gbc59d67          (12 Commits seit Tag v0.8, Hash bc59d67)
    //   v0.8-12-gbc59d67-dirty    (zusätzlich uncommittete Änderungen)
    //   bc59d67                   (Fallback ohne Tag)
    let version = git_describe().unwrap_or_else(|| "unbekannt".to_string());
    println!("cargo:rustc-env=LUXIFER_VERSION={version}");

    // Kurzer Commit-Hash separat, für die About-Anzeige.
    let hash = git_short_hash().unwrap_or_else(|| "-".to_string());
    println!("cargo:rustc-env=LUXIFER_COMMIT={hash}");

    // Bei jedem Commit neu bauen, damit die Version aktuell bleibt. HEAD zeigt
    // je nach Branch-Zustand auf packed-refs oder eine lose Ref — beide watchen.
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../.git/refs");
    println!("cargo:rerun-if-changed=../../../.git/packed-refs");

    tauri_build::build()
}

fn git_describe() -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;
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

fn git_short_hash() -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
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
