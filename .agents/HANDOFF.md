# PiGradient Companion Windows System Tray App — Agent Handoff

## 1. App Goal & Strict Invariants
**PiGradient Tray** is a lightweight, **read-only**, **local-LAN-only**, **100% pure Rust** Windows 10/11 system tray companion and taskbar-anchored flyout widget (`360 px` Material 3 dark card) for self-hosted **PiGradient** air quality gateways (default port `:8080`). Zero `npm`, zero Node.js, and zero Edge WebView2 are required to build, install (`cargo install --path .`), or run the Windows app.

### Non-Negotiable Behavioral Rules
1. **Strictly Read-Only Companion**: Never add LED tests, LED schedules, calibration triggers, restart buttons, or write API calls (`POST`/`PUT`/`DELETE`). The only gateway action is **`Open Web Dashboard`**, which opens `gatewayUrl` in the user's default browser via the `open` crate.
2. **Zero Cloud / Zero Tracking**: Communicates solely with the user-configured LAN gateway (`http://...` or `https://...`). Zero cloud telemetry, zero external font/CDN requests (no Google Fonts), and zero analytics.
3. **Zero Simulated or Fake Numbers**: Never fabricate fallback telemetry (`0 AQI`, `"Good"`, fake device models/serials). When unconfigured, offline, or `timestamp <= 0`, show `--` placeholders and neutral slate `#64748B` accents.
4. **Zero Idle GUI CPU**: Launches hidden in the system tray (`with_visible(false)`). While hidden, `eframe` runs zero `App::ui` rendering passes; `poller.rs` only polls `GET /api/v1/widget` on `idlePollSeconds` (default `30s`, clamped `15–120s`) and calls `ctx.request_repaint()` so `App::logic` updates the diff-cached Win32 tray icon/tooltip on the main UI thread. Active `1 Hz` SSE (`/api/v1/stream`) + `1h` `/api/v1/history` only run while `flyout_visible == true`.
5. **Pass-Through Gateway Semantics**: Do not implement client-side EPA AQI calculations or toast/alert thresholds. Display `processed.aqi_pm25`, `processed.aqi_category`, `processed.aqi_color`, and `processed.co2_category` directly from the gateway.

---

## 2. Repository Structure & File Map

### Pure Rust Windows Application (`/Cargo.toml`, `/build.rs`, `/src/`, `/assets/`)
| File | Responsibility |
| :--- | :--- |
| `/Cargo.toml` | Standard Cargo manifest (`pigradient-tray` v0.1.0, `edition = "2024"`, `MIT OR Apache-2.0`, default `src/main.rs` entrypoint). Dependencies: `eframe 0.37` (`glow`, `accesskit`, `clipboard`, `default_fonts`), `tray-icon 0.26` (`muda`), `reqwest 0.13` (`native-tls`, `json`, `stream`), `tokio 1`, `windows 0.62`, `single-instance 0.3`, `open 5`, `dirs 7`, `chrono 0.4`, `serde`/`serde_json`, `[build-dependencies] winresource 0.1`. Release profile (`opt-level = "s"`, `lto = "thin"`, `codegen-units = 1`, `strip = true`). |
| `/build.rs` | Embeds `/assets/icon.ico` and Windows Explorer metadata (`ProductName`, `CARGO_PKG_DESCRIPTION`) when `CARGO_CFG_TARGET_OS == "windows"`. |
| `/assets/*` | `icon.ico`, `icon.svg`, `32x32.png`, `128x128.png`, `128x128@2x.png`. |
| `/src/main.rs` | `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, `SingleInstance` lock, 2-thread Tokio runtime, hidden borderless transparent always-on-top `eframe` flyout (`clear_color = [0,0,0,0]`, `Renderer::Glow`, `system_font_fallback: false`), `TrayController`, `poller`, and hidden `WM_POWERBROADCAST` (`PBT_APMRESUMEAUTOMATIC`) Win32 wake listener via `OnceLock`. |
| `/src/constants.rs` | Centralizes Cargo-derived package constants (`SINGLE_INSTANCE_ID`, `USER_AGENT` via `CARGO_PKG_*`), app identity, Windows registry/window class names, polling/HTTP/SSE/history timings, flyout dimensions, tray icon constraints, Material 3 dark palette, and CO₂/VOC thresholds. |
| `/src/config.rs` | Loads/saves `%APPDATA%\PiGradient\settings.json` (`gatewayUrl`, `tempUnit`, `idlePollSeconds`, `pinFlyout`, `launchAtStartup`, `trayIconStyle`), normalizes LAN URLs (`http`/`https` only), and writes/deletes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\PiGradientTray` via `windows 0.62` Registry APIs. |
| `/src/poller.rs` | Two-mode async telemetry loop: idle `/api/v1/widget` poll vs. active `/api/v1/stream` 1 Hz SSE + throttled 30s `/api/v1/history`. Uses `#[serde(default)]` gateway structs, reconnect backoff (`2s -> 5s -> 15s`), and `std::sync::RwLock` for non-blocking UI reads. |
| `/src/tray.rs` | In-memory `32×32` RGBA icon generator (`render_badge_rgba` Status Ring & `render_aqi_digits_rgba` numeric AQI badge), `127`-char tooltip clamping (`format_tooltip_text`), `tray-icon` + `muda` 4-item context menu (`Open / Hide Flyout`, `Open Web Dashboard`, `Settings...`, `Quit PiGradient`), and multi-monitor taskbar placement (`compute_flyout_logical_pos` via `MonitorFromPoint` + `GetMonitorInfoW` / `SPI_GETWORKAREA`). |
| `/src/theme.rs` | Local Windows font loader (`%WINDIR%\Fonts\segoeui.ttf` + `CascadiaMono.ttf`/`consola.ttf` prepended ahead of `egui` default fallback fonts), WCAG AA contrast lift (`ensure_readable_dark_text_color`), VOC thresholds, and CO₂ category helpers. |
| `/src/ui/mod.rs` | `PiGradientApp` (`eframe::App::logic` + `ui`): content-hugging `360px` card, Setup/Offline banners, footer CTA, `Enter` to save, `Esc` / blur auto-hide with 220ms focus grace. |
| `/src/ui/header.rs` | Header status dot, `PiGradient · <status>` label, and 4 vector action buttons (`Open Web Dashboard`, `Pin`, `Settings`, `Close`). |
| `/src/ui/tiles.rs` | 2×2 KPI grid (`PM2.5`, `CO₂` `400–1800 ppm` progress gauge, `Climate`, `VOCs`). |
| `/src/ui/sparkline.rs` | Interactive `Painter` `1h Trend · CO₂ & PM2.5` chart with hover scrub cursor & live readout. |
| `/src/ui/settings.rs` | `SettingsFormState` and inline Settings form (`Gateway URL`, `°F`/`°C`, `Idle Poll`, `Tray Icon Badge`, `Windows Startup`). |
| `/src/ui/icons.rs` | Pure-vector `Painter` strokes for header/offline icons. |

### Developer & CI/CD Tooling (`/.vscode/`, `/.github/`)
| File | Responsibility |
| :--- | :--- |
| `/.vscode/extensions.json` | Recommends `rust-lang.rust-analyzer`, `ms-vscode.cpptools`, `vadimcn.vscode-lldb`. |
| `/.vscode/tasks.json` | Cargo tasks: `cargo: build (debug)`, `cargo: build (release)`, `cargo: test`, `cargo: clippy`, `cargo: install local`. |
| `/.vscode/launch.json` | Debug configs (`cppvsdbg` + `CodeLLDB`) targeting `target/debug/pigradient-tray.exe`. |
| `/.github/dependabot.yml` | Weekly updates for `cargo` and `github-actions` (limit 10 open PRs each). |
| `/.github/workflows/ci.yml` | Runs `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features --workspace -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`, and `cargo build --release` on `windows-latest` (uploading `pigradient-tray.exe` artifact). |
| `/.github/workflows/cd.yml` | Triggers on semver tags (`[v]?[0-9]+.[0-9]+.[0-9]+`) and `workflow_dispatch`: builds release binary + `.sha256` into `target/release-dist/`, uploads to GitHub Release, and publishes to `crates.io` if `CARGO_REGISTRY_TOKEN` is set. |
| `/.github/workflows/auto-release.yml` | Runs Sundays 00:00 UTC + `workflow_dispatch`: checks commits since latest tag, waits for status checks, bumps patch version in `Cargo.toml`, updates `CHANGELOG.md` via `git-cliff`, runs `rustsec/audit-check`, tests, fmt, clippy, docs, and release build, commits & tags `v<ver>`, creates GitHub Release with binary assets, and publishes to `crates.io` if `CARGO_REGISTRY_TOKEN` is set. |
| `/.github/workflows/auto-merge.yml` | On `dependabot[bot]` PRs: runs `cargo clippy --fix` + `cargo fmt --all`, pushes mechanical fixes if changed, verifies fmt/clippy/test/release build on `windows-latest`, enables `gh pr merge --auto --merge` on `semver-patch`, and comments on PR failure with run logs. |
| `/.github/workflows/audit.yml` | Runs `rustsec/audit-check@v2.0.0` every 4 days and on `Cargo.toml`/`Cargo.lock` pushes & PRs. |

### AI Studio Scaffolding
- The Vite/React web harness and the AI Studio `package.json` / `metadata.json` stubs have been removed; the repository is a pure Cargo project.

---

## 3. PiGradient Gateway API Contract & Local Storage

### LAN Gateway Endpoints
- `GET /api/v1/widget`: Lightweight idle summary (`{ aqi, aqi_desc, aqi_color, co2, temp, temp_c, humidity, updated }`). `updated == "--"` or `aqi_desc == "No Data"` indicates no telemetry.
- `GET /api/v1/live` & `GET /api/v1/stream` (`text/event-stream` 1 Hz SSE): Full telemetry `{ status, is_simulated, error_message, last_seen_seconds_ago, timestamp, latency_ms, device, raw, processed }`. Treat `timestamp <= 0` as no telemetry.
- `GET /api/v1/history?start=<unix>&end=<unix>&interval=60`: Chronological array of `{ timestamp, pm01, pm02_comp, pm10, rco2, atmp_comp, rhum_comp, tvoc_index, nox_index }`.

### Local Persistence & Windows OS Integration
- **Config File**: `%APPDATA%\PiGradient\settings.json` (`camelCase`: `gatewayUrl`, `tempUnit`, `idlePollSeconds`, `pinFlyout`, `launchAtStartup`, `trayIconStyle`).
- **Windows Startup**: `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` -> `"PiGradientTray"` = `"\"<exe_path>\""` (no CLI flag needed since the app always starts hidden in the tray).

---

## 4. Material 3 Visual Spec & Engineering Decisions

### Palette & Geometry
- **Outer card**: `360 px` wide, `#111318` fill, `12px` corner radius, `1px` `rgba(255,255,255,0.10)` border, content-hugging dynamic height (`ViewportCommand::InnerSize` + re-anchored `OuterPosition`).
- **Header**: `44 px` height, `#14171D` fill, bottom `1px` hairline `rgba(255,255,255,0.08)`, `10px` status dot + `PiGradient · <status>` + 4 vector `28×28 px` action buttons.
- **Tiles & Cards**: `#181B22` fill, `8px` radius, `1px` `rgba(255,255,255,0.08)` border; `#2563EB` primary buttons; `#60A5FA` focus ring & CO₂ sparkline stroke; `#10B981` PM2.5 sparkline stroke; `#F87171` validation error text.
- **WCAG AA Contrast Guard (`theme::ensure_readable_dark_text_color`)**: Tray icon uses raw gateway hex (`aqi_color`), whereas tile labels lift dark colors (e.g. `#7F1D1D -> #F87171`, `#8F3F97 -> #C084FC`) when relative luminance `< 0.22` against `#181B22`.

### Key Architectural Choices
1. **Main-Thread Tray Ownership (`App::logic` + `App::ui`)**: Win32 `Shell_NotifyIconW` requires main-thread calls. Background `poller.rs` updates `SharedAppState` and wakes `ctx.request_repaint()`; `App::logic` runs even when hidden to drain `TrayAction` and update diff-cached icon/tooltip while `App::ui` returns immediately when `!self.visible` (0% idle render cost).
2. **`3×7 @2x` Compact Glyphs for `>99` AQI**: `render_aqi_digits_rgba` uses `5×7 @2x` (`10×14`) for 1–2 digits (`0–99`) and `3×7 @2x` (`6×14` with crisp `2px` strokes) for 3 digits (`100–999` clamped) so Win32 16×16 DIP downsampling stays crisp.
3. **Windows SChannel (`native-tls`)**: Avoids CMake/NASM build toolchain dependencies (`aws-lc-rs`) so `cargo install` works out-of-the-box with MSVC Build Tools.
4. **220ms Focus-Loss Grace Guard**: `seen_focused_while_shown || shown_at.elapsed() > 220ms` avoids focus-bounce auto-hide during taskbar tray icon click activation while ensuring clean blur-to-hide afterward.
5. **CI Staging Inside `target/release-dist/`**: Both `cd.yml` and `auto-release.yml` copy binaries inside `target/` so `cargo publish` sees a clean git working tree, and `auto-release.yml` uploads GitHub Release assets directly because `GITHUB_TOKEN` tag pushes do not re-trigger `cd.yml`.

---

## 5. Known Assumptions & Hardcoded Constraints
- **Device Model Agnostic**: `device.model` and `device.firmware` are read dynamically from `/api/v1/live` and fall back to `"Read-only companion"` when invalid/unconfigured.
- **VOC/NOx Cutoffs (`theme::get_voc_display`)**: Assumes Sensirion SGP41 index bands (`150/250/400` TVOC, `5/20/100` NOx; `tvoc_index <= 0` treated as `"No Data"`). Swap to pass-through if the gateway adds `processed.voc_category`.
- **CO₂ Category Enum & Gauge**: Expects `optimal | fair | poor | critical`; progress bar clamps `400–1800 ppm`.
- **`#[serde(default)]` Defaults**: Missing gateway numeric fields default to `0.0`. If a gateway has `timestamp > 0` without a CO₂ sensor, `rco2 == 0` renders `0`.
- **History Query Window**: Hardcoded `now - 3600` at `interval=60` plotting `rco2` and `pm02_comp`.
- **Single-Instance Lock**: Second launch exits silently instead of opening the already-running instance's flyout (no named-pipe IPC between instances yet).

---

## 6. GitHub Setup & Remaining Windows Verification

### Required GitHub Repository Settings (for `.github/` Workflows)
1. **Settings -> General -> Pull Requests**: Enable **Allow auto-merge**.
2. **Settings -> Actions -> General -> Workflow permissions**: Select **Read and write permissions**.
3. **Settings -> Secrets and variables -> Actions** *(for `crates.io` auto-publish)*: Add `CARGO_REGISTRY_TOKEN`.
4. Before publishing to `crates.io`: add `repository = "https://github.com/<owner>/<repo>"` and `readme = "README.md"` in `/Cargo.toml` if desired.

### Verification Status
- **Verified in Linux container**:
  - `npm run lint` and `npm run build` (verify `Cargo.toml`, `src/main.rs`, `src/constants.rs`, and `src/ui/mod.rs` layout).
  - Exact API signatures checked against `crates.io` / `docs.rs` / `windows-docs-rs` (`eframe 0.37.0`, `egui 0.37.0`, `tray-icon 0.26.1`, `reqwest 0.13.5`, `dirs 7.0.0`, `windows 0.62`, `single-instance 0.3.3`, `open 5.4.5`, `winresource 0.1.34`).
- **NOT yet run (requires Windows 10/11 or Windows CI runner — `rustc`/`cargo` absent in container)**:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --all-targets --all-features --workspace -- -D warnings`
  3. `cargo test --all-features --workspace`
  4. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace`
  5. `cargo build --release` / `cargo install --path .`
  6. Manual runtime checks on Windows 10 & 11:
     - Hidden startup (`0%` UI pass while closed), Explorer/shortcut icon from `build.rs`
     - Left-click tray anchoring across bottom/top/side taskbars, multi-monitor, and `100%`–`200%` DPI
     - Focus-loss auto-hide, pin button, `Esc` behavior, `Status Ring` vs `AQI #` (including `>99`)
     - Hover sparkline cursor scrubbing, live SSE (`1 Hz`), offline retry, `HKCU\...\Run` toggle, and sleep/wake reconnect (`PBT_APMRESUMEAUTOMATIC`).
