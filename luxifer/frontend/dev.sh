#!/usr/bin/env bash
# LuxiFer im Dev-Modus starten.
#
# GDK_BACKEND=x11 (über XWayland statt nativem Wayland): messbar geringere
# Eingabe-Latenz. Der frühere Wayland-Pfad brauchte WEBKIT_DISABLE_COMPOSITING_MODE
# (sonst leeres Fenster), was das Canvas-Present über einen langsamen Software-Pfad
# zwang und den "Cursor klebt"-Versatz verursachte. Unter X11 rendert das Fenster
# mit vollem HW-Compositing (DMABUF an) — der Versatz ist deutlich geringer und der
# Workaround-Flag entfällt. (Diagnose 2026-07-11: Latenz lag in der WebKit/Wayland-
# Present-Pipeline, nicht im App-Code; per rohem Cursor-Kreuz nachgewiesen.)
set -e
cd "$(dirname "$0")"

export GDK_BACKEND=x11

npm run tauri dev
