use eframe::egui::Color32;

// -----------------------------------------------------------------------------
// Application Identity & Windows System Integration
// -----------------------------------------------------------------------------

pub const APP_NAME: &str = "PiGradient";
pub const SINGLE_INSTANCE_ID: &str = concat!(env!("CARGO_PKG_NAME"), "-single-instance");
pub const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
pub const CONFIG_DIR_NAME: &str = "PiGradient";
pub const CONFIG_FILE_NAME: &str = "settings.json";

#[cfg(windows)]
pub const STARTUP_REG_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0";
#[cfg(windows)]
pub const STARTUP_REG_VALUE_NAME: &str = "PiGradientTray\0";
#[cfg(windows)]
pub const POWER_LISTENER_CLASS_NAME: &str = "PiGradientPowerListener\0";

// -----------------------------------------------------------------------------
// Settings Defaults & Telemetry Polling Timings
// -----------------------------------------------------------------------------

pub const DEFAULT_TEMP_UNIT: &str = "f";
pub const DEFAULT_TRAY_ICON_STYLE: &str = "ring";
pub const DEFAULT_IDLE_POLL_SECONDS: u64 = 30;
pub const MIN_IDLE_POLL_SECONDS: u64 = 15;
pub const MAX_IDLE_POLL_SECONDS: u64 = 120;
pub const IDLE_POLL_PRESETS: [u64; 3] = [15, 30, 60];

pub const HTTP_TIMEOUT_SECS: u64 = 5;
pub const SSE_CONNECT_TIMEOUT_SECS: u64 = 5;
pub const HISTORY_WINDOW_SECS: i64 = 3600;
pub const HISTORY_INTERVAL_SECS: i64 = 60;
pub const HISTORY_STALE_MS: i64 = 30_000;
pub const MAX_SPARKLINE_POINTS: usize = 60;

pub const BACKOFF_INITIAL_SECS: u64 = 2;
pub const BACKOFF_MED_SECS: u64 = 5;
pub const BACKOFF_MAX_SECS: u64 = 15;

// -----------------------------------------------------------------------------
// Flyout Window & Interaction Geometry
// -----------------------------------------------------------------------------

pub const FLYOUT_WIDTH: f32 = 360.0;
pub const FLYOUT_INITIAL_HEIGHT: f32 = 398.0;
pub const FLYOUT_MIN_HEIGHT: f32 = 260.0;
pub const FLYOUT_SHOW_MIN_HEIGHT: f32 = 300.0;
pub const FLYOUT_SCREEN_MARGIN: f32 = 8.0;
pub const FOCUS_LOSS_GRACE_MS: u128 = 220;

// -----------------------------------------------------------------------------
// System Tray Icon & Tooltip Constraints
// -----------------------------------------------------------------------------

pub const TRAY_ICON_SIZE: usize = 32;
pub const MAX_TOOLTIP_CHARS: usize = 127;
pub const DEFAULT_TRAY_TOOLTIP: &str = "PiGradient · Click to set gateway URL";
pub const DEFAULT_TRAY_BADGE_HEX: &str = "#2563EB";
pub const NEUTRAL_SLATE_HEX: &str = "#64748B";

// -----------------------------------------------------------------------------
// Material 3 Dark Palette & Sensor Thresholds
// -----------------------------------------------------------------------------

pub const SURFACE_CARD: Color32 = Color32::from_rgb(17, 19, 24); // #111318
pub const SURFACE_HEADER: Color32 = Color32::from_rgb(20, 23, 29); // #14171D
pub const SURFACE_TILE: Color32 = Color32::from_rgb(24, 27, 34); // #181B22
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 232, 238); // #E6E8EE
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(156, 163, 175); // #9CA3AF
pub const TEXT_MUTED: Color32 = Color32::from_rgb(141, 149, 165); // #8D95A5
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(37, 99, 235); // #2563EB
pub const ACCENT_FOCUS_CO2: Color32 = Color32::from_rgb(96, 165, 250); // #60A5FA
pub const ACCENT_PM25: Color32 = Color32::from_rgb(16, 185, 129); // #10B981
pub const ERROR_RED: Color32 = Color32::from_rgb(248, 113, 113); // #F87171

pub const NEUTRAL_SLATE_COLOR: Color32 = Color32::from_rgb(100, 116, 139); // #64748B
pub const STATUS_GREEN: Color32 = Color32::from_rgb(16, 185, 129); // #10B981
pub const STATUS_AMBER: Color32 = Color32::from_rgb(245, 158, 11); // #F59E0B
pub const STATUS_ORANGE: Color32 = Color32::from_rgb(249, 115, 22); // #F97316
pub const STATUS_RED: Color32 = Color32::from_rgb(244, 63, 94); // #F43F5E
pub const STATUS_SKY: Color32 = Color32::from_rgb(56, 189, 248); // #38BDF8
pub const CONTRAST_LIFT_PURPLE: Color32 = Color32::from_rgb(192, 132, 252); // #C084FC

pub const WCAG_AA_MIN_LUMINANCE: f64 = 0.22;
pub const CO2_GAUGE_MIN: f64 = 400.0;
pub const CO2_GAUGE_MAX: f64 = 1800.0;

pub const TVOC_HIGH: f64 = 400.0;
pub const TVOC_ELEVATED: f64 = 250.0;
pub const TVOC_MODERATE: f64 = 150.0;
pub const NOX_HIGH: f64 = 100.0;
pub const NOX_ELEVATED: f64 = 20.0;
pub const NOX_MODERATE: f64 = 5.0;
