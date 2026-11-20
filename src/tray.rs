use eframe::egui::{self, Pos2};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, Rect, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::config::AppSettings;
use crate::constants::{
    DEFAULT_TRAY_TOOLTIP, FLYOUT_SCREEN_MARGIN, MAX_TOOLTIP_CHARS, NEUTRAL_SLATE_HEX,
    TRAY_ICON_SIZE,
};

#[derive(Debug, Clone)]
pub enum TrayAction {
    TrayPointerDown(Option<Rect>),
    ToggleFlyout(Option<Rect>),
    OpenDashboard,
    OpenSettings,
    Quit,
}

pub fn parse_hex_rgb(hex: &str) -> (u8, u8, u8) {
    let cleaned = hex.trim().trim_start_matches('#');
    if cleaned.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&cleaned[0..2], 16),
            u8::from_str_radix(&cleaned[2..4], 16),
            u8::from_str_radix(&cleaned[4..6], 16),
        )
    {
        return (r, g, b);
    }
    (100, 116, 139) // Neutral slate #64748B
}

/// Renders a crisp 32x32 RGBA badge icon in memory.
/// Uses an anti-aliased dark slate bezel (#181B22) with an outer status ring
/// and bright center status dot matching `badge_hex`.
pub fn render_badge_rgba(badge_hex: &str) -> Vec<u8> {
    let size = TRAY_ICON_SIZE;
    let mut rgba = vec![0u8; size * size * 4];
    let (ar, ag, ab) = parse_hex_rgb(badge_hex);
    let cx = 15.5f32;
    let cy = 15.5f32;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = (y * size + x) * 4;

            if dist <= 14.5 {
                let edge_alpha = if dist > 13.5 {
                    ((14.5 - dist).clamp(0.0, 1.0) * 255.0) as u8
                } else {
                    255
                };

                if dist >= 10.6 {
                    rgba[idx] = ar;
                    rgba[idx + 1] = ag;
                    rgba[idx + 2] = ab;
                    rgba[idx + 3] = edge_alpha;
                } else if dist <= 5.2 {
                    rgba[idx] = ar;
                    rgba[idx + 1] = ag;
                    rgba[idx + 2] = ab;
                    rgba[idx + 3] = 255;
                } else {
                    rgba[idx] = 24;
                    rgba[idx + 1] = 27;
                    rgba[idx + 2] = 34;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    rgba
}

fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        _ => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
    }
}

fn glyph_3x7(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b001, 0b111, 0b100, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b001, 0b111, 0b001, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b101, 0b111, 0b001, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b100, 0b111, 0b001, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b100, 0b111, 0b101, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b101, 0b111, 0b101, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b101, 0b111, 0b001, 0b001, 0b111],
        _ => [0b000, 0b000, 0b000, 0b111, 0b000, 0b000, 0b000],
    }
}

fn srgb_channel_to_linear(c: u8) -> f32 {
    let v = c as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance (WCAG) of an sRGB colour, used to choose readable ink.
fn relative_luminance((r, g, b): (u8, u8, u8)) -> f32 {
    0.2126 * srgb_channel_to_linear(r)
        + 0.7152 * srgb_channel_to_linear(g)
        + 0.0722 * srgb_channel_to_linear(b)
}

fn contrast_ratio(a: f32, b: f32) -> f32 {
    let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

const INK_DARK: (u8, u8, u8) = (11, 18, 32); // #0B1220
const INK_LIGHT: (u8, u8, u8) = (255, 255, 255);

/// Picks whichever ink has the higher contrast against the badge fill.
/// A fixed white would vanish on amber; a fixed dark would muddy deep reds.
fn pick_ink(fill: (u8, u8, u8)) -> (u8, u8, u8) {
    let fill_lum = relative_luminance(fill);
    if contrast_ratio(fill_lum, relative_luminance(INK_DARK))
        >= contrast_ratio(fill_lum, relative_luminance(INK_LIGHT))
    {
        INK_DARK
    } else {
        INK_LIGHT
    }
}

/// Renders a 32x32 rounded square badge filled with the status colour and
/// containing numeric AQI digits.
///
/// The whole badge is filled rather than outlined: at 16x16 DIP a 2px outline
/// downsamples into a hollow box. Corners are antialiased via a rounded-box
/// SDF, and the digits use whichever ink contrasts with the fill.
/// Uses 5x7 @2x (10x14) for 1-2 chars and 3x7 @2x (6x14) for 3 chars so >99
/// stays full-height with 2px strokes when Windows downsamples to 16x16 DIP.
pub fn render_aqi_digits_rgba(badge_hex: &str, text: &str) -> Vec<u8> {
    let size = TRAY_ICON_SIZE;
    let mut rgba = vec![0u8; size * size * 4];
    let (fr, fg, fb) = parse_hex_rgb(badge_hex);
    let (ir, ig, ib) = pick_ink((fr, fg, fb));

    // Rounded-box signed distance field: 1px inset from the canvas edge, 7px
    // corner radius, one pixel of feathered edge.
    let center = size as f32 / 2.0;
    let half = center - 1.0;
    let radius = 7.0f32;
    let feather = 0.5;

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5 - center;
            let py = y as f32 + 0.5 - center;
            let qx = px.abs() - (half - radius);
            let qy = py.abs() - (half - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let inside = qx.max(qy).min(0.0);
            let distance = outside + inside - radius;
            let coverage = (feather - distance).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let idx = (y * size + x) * 4;
            rgba[idx] = fr;
            rgba[idx + 1] = fg;
            rgba[idx + 2] = fb;
            rgba[idx + 3] = (coverage * 255.0).round() as u8;
        }
    }

    let chars: Vec<char> = text.chars().take(3).collect();
    let compact = chars.len() >= 3;
    let scale = 2usize;
    let cols = if compact { 3usize } else { 5usize };
    let glyph_w = cols * scale;
    let glyph_h = 7 * scale;
    let gap = if chars.len() == 2 { 4usize } else { 2usize };
    let total_w = chars.len() * glyph_w + chars.len().saturating_sub(1) * gap;
    // Snap to even coordinates so 2px strokes align cleanly to 16x16 DIP pixels.
    let start_x = ((size.saturating_sub(total_w)) / 2) & !1;
    let start_y = ((size.saturating_sub(glyph_h)) / 2) & !1;

    for (ci, ch) in chars.iter().enumerate() {
        let rows = if compact {
            glyph_3x7(*ch)
        } else {
            glyph_5x7(*ch)
        };
        let gx = start_x + ci * (glyph_w + gap);
        for (ry, row_bits) in rows.iter().enumerate() {
            for rx in 0..cols {
                if (row_bits >> (cols - 1 - rx)) & 1 == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = gx + rx * scale + sx;
                            let py = start_y + ry * scale + sy;
                            if px < size && py < size {
                                let idx = (py * size + px) * 4;
                                if rgba[idx + 3] > 0 {
                                    rgba[idx] = ir;
                                    rgba[idx + 1] = ig;
                                    rgba[idx + 2] = ib;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    rgba
}

pub fn clamp_tooltip(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_TOOLTIP_CHARS {
        return text.to_string();
    }
    let truncated: String = text.chars().take(MAX_TOOLTIP_CHARS - 1).collect();
    format!("{truncated}…")
}

#[allow(clippy::too_many_arguments)] // flat formatter for optional sensor fields
pub fn format_tooltip_text(
    connection_state: &str,
    aqi: Option<f64>,
    aqi_desc: Option<&str>,
    co2: Option<f64>,
    temp_f: Option<f64>,
    temp_c: Option<f64>,
    rh: Option<f64>,
    last_seen_sec: Option<i64>,
    settings: &AppSettings,
) -> String {
    if settings.gateway_url.trim().is_empty() || connection_state == "unconfigured" {
        return clamp_tooltip("PiGradient · Click to set gateway URL");
    }
    if connection_state == "offline" {
        let seen = match last_seen_sec {
            Some(s) if (0..60).contains(&s) => format!(" (last seen {s}s ago)"),
            Some(s) if (60..3600).contains(&s) => format!(" (last seen {}m ago)", s / 60),
            Some(s) if s >= 3600 => format!(" (last seen {}h ago)", s / 3600),
            _ => String::new(),
        };
        return clamp_tooltip(&format!("PiGradient · Monitor offline{seen}"));
    }
    match (aqi, aqi_desc, co2, temp_f, temp_c, rh) {
        (Some(a), Some(desc), Some(c), Some(tf), Some(tc), Some(h)) => {
            let temp = if settings.temp_unit == "c" {
                format!("{tc:.1}°C")
            } else {
                format!("{tf:.1}°F")
            };
            clamp_tooltip(&format!(
                "PiGradient · AQI {:.0} ({desc}) · CO₂ {:.0} ppm · {temp} · {:.0}% RH",
                a, c, h
            ))
        }
        _ => clamp_tooltip("PiGradient · Waiting for sensor telemetry"),
    }
}

pub struct TrayController {
    tray: TrayIcon,
    last_icon_key: String,
    last_tooltip: String,
}

impl TrayController {
    pub fn create(
        action_tx: std::sync::mpsc::Sender<TrayAction>,
        egui_ctx: egui::Context,
    ) -> Result<Self, String> {
        let toggle_item = MenuItem::with_id("toggle_flyout", "Open / Hide Flyout", true, None);
        let dashboard_item = MenuItem::with_id("open_dashboard", "Open Web Dashboard", true, None);
        let settings_item = MenuItem::with_id("open_settings", "Settings...", true, None);
        let sep = PredefinedMenuItem::separator();
        let quit_item = MenuItem::with_id("quit", "Quit PiGradient", true, None);

        let menu = Menu::with_items(&[
            &toggle_item,
            &dashboard_item,
            &settings_item,
            &sep,
            &quit_item,
        ])
        .map_err(|e| format!("Menu build failed: {e}"))?;

        let icon = Icon::from_rgba(
            render_badge_rgba(NEUTRAL_SLATE_HEX),
            TRAY_ICON_SIZE as u32,
            TRAY_ICON_SIZE as u32,
        )
        .map_err(|e| format!("Tray icon creation failed: {e}"))?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(DEFAULT_TRAY_TOOLTIP)
            .with_icon(icon)
            .build()
            .map_err(|e| format!("TrayIconBuilder failed: {e}"))?;

        let tx_click = action_tx.clone();
        let ctx_click = egui_ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                rect,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                rect,
                ..
            } => {
                let _ = tx_click.send(TrayAction::TrayPointerDown(Some(rect)));
                ctx_click.request_repaint();
            }
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => {
                let _ = tx_click.send(TrayAction::ToggleFlyout(Some(rect)));
                ctx_click.request_repaint();
            }
            _ => {}
        }));

        let tx_menu = action_tx;
        let ctx_menu = egui_ctx;
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let act = match event.id().as_ref() {
                "toggle_flyout" => Some(TrayAction::ToggleFlyout(None)),
                "open_dashboard" => Some(TrayAction::OpenDashboard),
                "open_settings" => Some(TrayAction::OpenSettings),
                "quit" => Some(TrayAction::Quit),
                _ => None,
            };
            if let Some(a) = act {
                let _ = tx_menu.send(a);
                ctx_menu.request_repaint();
            }
        }));

        Ok(Self {
            tray,
            last_icon_key: String::new(),
            last_tooltip: String::new(),
        })
    }

    pub fn update_visuals(
        &mut self,
        badge_hex: &str,
        icon_style: &str,
        aqi_text: Option<&str>,
        tooltip: &str,
    ) {
        let text_label = aqi_text.unwrap_or("--");
        let cache_key = format!("{icon_style}:{badge_hex}:{text_label}");
        if self.last_icon_key != cache_key {
            let pixels = if icon_style == "aqi" {
                render_aqi_digits_rgba(badge_hex, text_label)
            } else {
                render_badge_rgba(badge_hex)
            };
            if let Ok(icon) = Icon::from_rgba(pixels, TRAY_ICON_SIZE as u32, TRAY_ICON_SIZE as u32)
            {
                let _ = self.tray.set_icon(Some(icon));
            }
            self.last_icon_key = cache_key;
        }

        let clamped = clamp_tooltip(tooltip);
        if self.last_tooltip != clamped {
            let _ = self.tray.set_tooltip(Some(clamped.clone()));
            self.last_tooltip = clamped;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[cfg(windows)]
pub fn work_area_for_point(px: i32, py: i32) -> PhysicalRect {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };

    unsafe {
        let hmon = MonitorFromPoint(POINT { x: px, y: py }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return PhysicalRect {
                left: mi.rcWork.left,
                top: mi.rcWork.top,
                right: mi.rcWork.right,
                bottom: mi.rcWork.bottom,
            };
        }

        let mut work = RECT::default();
        if SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
        {
            return PhysicalRect {
                left: work.left,
                top: work.top,
                right: work.right,
                bottom: work.bottom,
            };
        }
    }

    PhysicalRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1032,
    }
}

#[cfg(not(windows))]
pub fn work_area_for_point(_px: i32, _py: i32) -> PhysicalRect {
    PhysicalRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1032,
    }
}

/// Effective DPI scale (`dpi / 96.0`) of the monitor containing physical
/// `(x, y)`.
///
/// The parked window holds a frame laid out at the scale it was last painted
/// with. Revealing it on a monitor with a different scale would show that
/// frame at the wrong size, so the reveal path compares this against
/// `pixels_per_point()` and falls back to the cloaked settle when they differ.
#[cfg(windows)]
pub fn monitor_dpi_scale_at(x: f32, y: f32) -> Option<f32> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    unsafe {
        let hmon = MonitorFromPoint(
            POINT {
                x: x as i32,
                y: y as i32,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)
            .ok()
            .filter(|()| dpi_x > 0)
            .map(|()| dpi_x as f32 / 96.0)
    }
}

#[cfg(not(windows))]
pub fn monitor_dpi_scale_at(_x: f32, _y: f32) -> Option<f32> {
    None
}

/// Computes logical `OuterPosition` for the flyout card anchored cleanly above
/// (or beside, on vertical taskbars) the clicked tray icon and clamped strictly
/// inside the monitor's usable work area with an 8pt safety margin.
pub fn compute_flyout_logical_pos(
    tray_rect: Option<Rect>,
    flyout_logical_w: f32,
    flyout_logical_h: f32,
    pixels_per_point: f32,
    work_area_phys: PhysicalRect,
) -> Pos2 {
    let ppp = pixels_per_point.max(0.5);
    let margin = FLYOUT_SCREEN_MARGIN;

    let wa_left = work_area_phys.left as f32 / ppp;
    let wa_top = work_area_phys.top as f32 / ppp;
    let wa_right = work_area_phys.right as f32 / ppp;
    let wa_bottom = work_area_phys.bottom as f32 / ppp;

    let (raw_x, raw_y) = if let Some(rect) = tray_rect {
        let icon_left = rect.position.x as f32 / ppp;
        let icon_top = rect.position.y as f32 / ppp;
        let icon_right = (rect.position.x as f32 + rect.size.width as f32) / ppp;
        let icon_bottom = (rect.position.y as f32 + rect.size.height as f32) / ppp;
        let icon_center_x = (icon_left + icon_right) * 0.5;
        let icon_center_y = (icon_top + icon_bottom) * 0.5;

        // The tray icon lives in the taskbar strip, which sits outside the
        // monitor's work area. Which edge that strip is on tells us where the
        // flyout goes: beside the icon for a vertical taskbar, above or below
        // it for a horizontal one.
        if icon_center_x < wa_left {
            (icon_right + margin, icon_center_y - flyout_logical_h * 0.5)
        } else if icon_center_x > wa_right {
            (
                icon_left - flyout_logical_w - margin,
                icon_center_y - flyout_logical_h * 0.5,
            )
        } else if icon_center_y < wa_top {
            (icon_center_x - flyout_logical_w * 0.5, icon_bottom + margin)
        } else if icon_center_y > wa_bottom {
            (
                icon_center_x - flyout_logical_w * 0.5,
                (wa_bottom - flyout_logical_h - margin).min(icon_top - flyout_logical_h - margin),
            )
        } else {
            // Icon reported inside the work area (auto-hidden or floating
            // taskbar): prefer the usual above-the-icon placement.
            (
                icon_center_x - flyout_logical_w * 0.5,
                (wa_bottom - flyout_logical_h - margin).min(icon_top - flyout_logical_h - margin),
            )
        }
    } else {
        (
            wa_right - flyout_logical_w - margin,
            wa_bottom - flyout_logical_h - margin,
        )
    };

    let min_x = wa_left + margin;
    let max_x = (wa_right - flyout_logical_w - margin).max(min_x);
    let min_y = wa_top + margin;
    let max_y = (wa_bottom - flyout_logical_h - margin).max(min_y);

    Pos2::new(raw_x.clamp(min_x, max_x), raw_y.clamp(min_y, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_beside_the_icon_on_vertical_taskbars() {
        let work = PhysicalRect {
            left: 0,
            top: 0,
            right: 1900,
            bottom: 1032,
        };
        // Right-hand taskbar: icon sits past the work area's right edge.
        let icon = Rect {
            position: tray_icon::dpi::PhysicalPosition::new(1904.0, 500.0),
            size: tray_icon::dpi::PhysicalSize::new(16, 24),
        };
        let pos = compute_flyout_logical_pos(Some(icon), 360.0, 396.0, 1.0, work);
        assert!(
            pos.x + 360.0 <= 1900.0 - 8.0 + 0.5,
            "flyout overlaps taskbar: {}",
            pos.x
        );
        assert!(pos.x >= 8.0);

        // Left-hand taskbar: work area starts after the strip.
        let work_left = PhysicalRect {
            left: 40,
            top: 0,
            right: 1920,
            bottom: 1032,
        };
        let icon_left = Rect {
            position: tray_icon::dpi::PhysicalPosition::new(8.0, 500.0),
            size: tray_icon::dpi::PhysicalSize::new(16, 24),
        };
        let pos = compute_flyout_logical_pos(Some(icon_left), 360.0, 396.0, 1.0, work_left);
        assert!(
            pos.x >= 40.0 + 8.0 - 0.5,
            "flyout overlaps taskbar: {}",
            pos.x
        );
        assert!(pos.x + 360.0 <= 1920.0);
    }

    #[test]
    fn clamps_tooltip_at_127_chars() {
        let long = "a".repeat(200);
        let clamped = clamp_tooltip(&long);
        assert_eq!(clamped.chars().count(), MAX_TOOLTIP_CHARS);
        assert!(clamped.ends_with('…'));
    }

    #[test]
    fn renders_filled_badge_with_transparent_corners() {
        let rgba = render_aqi_digits_rgba("#10B981", "42");
        assert_eq!(rgba.len(), 32 * 32 * 4);
        let px = |x: usize, y: usize| {
            let i = (y * 32 + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        // Outside the rounded square: fully transparent, no grey box corner.
        assert_eq!(px(0, 0)[3], 0);
        assert_eq!(px(31, 31)[3], 0);
        // Edge midpoints are solidly filled with the status colour.
        assert_eq!(px(16, 1), [0x10, 0xB9, 0x81, 255]);
        assert_eq!(px(1, 16), [0x10, 0xB9, 0x81, 255]);
        // The interior is filled too, not a dark bezel.
        assert_eq!(px(16, 4), [0x10, 0xB9, 0x81, 255]);
    }

    #[test]
    fn picks_readable_ink_for_each_fill() {
        // Bright fills get dark ink, dark fills keep white ink.
        assert_eq!(pick_ink((0xF5, 0x9E, 0x0B)), INK_DARK);
        assert_eq!(pick_ink((0x64, 0x74, 0x8B)), INK_LIGHT);
    }

    #[test]
    fn renders_three_digit_aqi_digits_in_contrasting_ink() {
        let rgba = render_aqi_digits_rgba("#F59E0B", "145");
        let ink_pixels = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| (px[0], px[1], px[2], px[3]) == (11, 18, 32, 255))
            .count();
        assert!(ink_pixels > 40);
    }

    #[test]
    fn renders_two_digit_aqi_digits_aligned_to_2x2_grid() {
        let rgba = render_aqi_digits_rgba("#10B981", "15");
        let is_ink = |x: usize, y: usize| {
            let i = (y * 32 + x) * 4;
            (rgba[i], rgba[i + 1], rgba[i + 2]) == INK_DARK
        };
        for by in 0..16 {
            for bx in 0..16 {
                let c00 = is_ink(bx * 2, by * 2);
                let c10 = is_ink(bx * 2 + 1, by * 2);
                let c01 = is_ink(bx * 2, by * 2 + 1);
                let c11 = is_ink(bx * 2 + 1, by * 2 + 1);
                assert!(
                    c00 == c10 && c00 == c01 && c00 == c11,
                    "2x2 block ({bx},{by}) should be uniform for crisp 16x16 downsampling"
                );
            }
        }
    }

    #[test]
    fn anchors_flyout_above_bottom_tray_within_work_area() {
        let rect = Rect {
            position: tray_icon::dpi::PhysicalPosition::new(1780.0, 1040.0),
            size: tray_icon::dpi::PhysicalSize::new(24, 36),
        };
        let work = PhysicalRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1032,
        };
        let pos = compute_flyout_logical_pos(Some(rect), 360.0, 396.0, 1.0, work);
        assert!(pos.x >= 8.0 && pos.x <= 1920.0 - 360.0 - 8.0);
        assert!((pos.y - (1032.0 - 396.0 - 8.0)).abs() < 1.0);
    }
}
