# PiGradient Companion Windows System Tray App

Read-only, zero-idle-overhead, **100% pure Rust** Windows 10/11 system tray companion and taskbar-anchored widget flyout (`360 px` Material 3 dark card) for local **PiGradient** air quality gateways.

No `npm`, no Node.js, and no Edge WebView2 required.

## Install & Run (Pure Cargo)
```powershell
cargo install --path .
pigradient-tray
```
Or build directly from source:
```powershell
cargo build --release
.\target\release\pigradient-tray.exe
```

On first launch, **PiGradient** sits quietly in the Windows system tray. Left-click the tray icon -> click **Set URL** -> enter your PiGradient gateway LAN URL (for example `http://raspberrypi.local:8080`) -> click **Save**.

## Module Layout (`src/`)
- `src/main.rs` — single-instance lock, Tokio runtime, hidden borderless `eframe` flyout, Win32 `WM_POWERBROADCAST` wake listener
- `src/constants.rs` — centralized application identity, polling intervals, flyout geometry, Material 3 dark palette, and sensor thresholds
- `src/config.rs` — `%APPDATA%\PiGradient\settings.json` persistence, LAN URL validation, `HKCU\...\Run` startup toggle
- `src/poller.rs` — idle `/api/v1/widget` polling (`15–120s`, default `30s`) vs. active `1 Hz` SSE (`/api/v1/stream`) + `1h` `/api/v1/history`
- `src/tray.rs` — in-memory `32×32` RGBA status ring & numeric AQI badge (`3×7 @2x` for `>99`), `127`-char tooltip, Win32 monitor work-area positioning
- `src/theme.rs` — local Windows font loader (`Segoe UI` + `Cascadia Mono` / `Consolas`), WCAG AA contrast guard, VOC/CO₂ helpers (zero external fonts/CDNs)
- `src/ui/` — modular `egui` flyout (`mod.rs`, `header.rs`, `tiles.rs`, `sparkline.rs`, `settings.rs`, `icons.rs`)

## Prerequisites on Windows 10 / 11
- [Rustup](https://rustup.rs/) (`stable-x86_64-pc-windows-msvc`) + Visual Studio Build Tools (C++ workload / Windows SDK)

## Debugging in VS Code (`.vscode/`)
1. Open the workspace folder in VS Code and install recommended extensions (`.vscode/extensions.json`).
2. Press **F5** with **Debug PiGradient Tray (Windows MSVC - cppvsdbg)** (or **CodeLLDB**), or use **Terminal -> Run Task**:
   - `cargo: test` — runs pure Rust unit tests
   - `cargo: clippy` — runs Clippy (`-D warnings`)
   - `cargo: build (release)` — compiles `target/release/pigradient-tray.exe` with embedded `assets/icon.ico`

## Agent Handoff
See `.agents/HANDOFF.md` for architecture, API contracts, CI/CD setup, and engineering decisions.
