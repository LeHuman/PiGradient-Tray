use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Stroke};
use std::path::PathBuf;
use std::sync::Arc;

use crate::constants::{
    ACCENT_FOCUS_CO2, ACCENT_PRIMARY, CONTRAST_LIFT_PURPLE, CO2_GAUGE_MAX, CO2_GAUGE_MIN,
    ERROR_RED, NEUTRAL_SLATE_COLOR, NOX_ELEVATED, NOX_HIGH, NOX_MODERATE, STATUS_AMBER,
    STATUS_GREEN, STATUS_ORANGE, STATUS_RED, STATUS_SKY, SURFACE_TILE, TEXT_PRIMARY,
    TEXT_SECONDARY, TVOC_ELEVATED, TVOC_HIGH, TVOC_MODERATE, WCAG_AA_MIN_LUMINANCE,
};

pub fn border_card() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 26) // white / 0.10
}

pub fn border_subtle() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 20) // white / 0.08
}

pub fn border_input() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 31) // white / 0.12
}

pub fn hover_overlay() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 20) // white / 0.08
}

pub fn active_tint_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(37, 99, 235, 64) // #2563EB / 0.25
}

pub fn parse_color32(hex: &str) -> Color32 {
    let cleaned = hex.trim().trim_start_matches('#');
    if cleaned.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&cleaned[0..2], 16),
            u8::from_str_radix(&cleaned[2..4], 16),
            u8::from_str_radix(&cleaned[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    NEUTRAL_SLATE_COLOR
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Ensures gateway-supplied status hex colors (e.g. EPA Hazardous #7F1D1D or
/// Very Unhealthy #8F3F97) remain WCAG AA readable on dark #181B22 tile cards.
pub fn ensure_readable_dark_text_color(hex: &str) -> Color32 {
    let cleaned = hex.trim().trim_start_matches('#');
    if cleaned.len() != 6 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return TEXT_SECONDARY;
    }
    let Ok(ri) = u8::from_str_radix(&cleaned[0..2], 16) else {
        return TEXT_SECONDARY;
    };
    let Ok(gi) = u8::from_str_radix(&cleaned[2..4], 16) else {
        return TEXT_SECONDARY;
    };
    let Ok(bi) = u8::from_str_radix(&cleaned[4..6], 16) else {
        return TEXT_SECONDARY;
    };

    let r = ri as f64 / 255.0;
    let g = gi as f64 / 255.0;
    let b = bi as f64 / 255.0;
    let lum = 0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b);

    // Dark surface #181B22 luminance is ~0.0108; lum >= 0.22 ensures > 4.5:1 AA contrast
    if lum >= WCAG_AA_MIN_LUMINANCE {
        return Color32::from_rgb(ri, gi, bi);
    }
    if r > g && r > b && b <= g {
        return ERROR_RED;
    }
    if r > g && b > g {
        return CONTRAST_LIFT_PURPLE;
    }
    if r > g && r > b {
        return ERROR_RED;
    }
    TEXT_SECONDARY
}

pub fn get_co2_category_display(category: &str) -> (&'static str, Color32) {
    match category {
        "optimal" => ("Good", STATUS_GREEN),
        "fair" => ("Moderate", STATUS_AMBER),
        "poor" => ("Elevated", STATUS_ORANGE),
        "critical" => ("High", STATUS_RED),
        _ => ("No Data", NEUTRAL_SLATE_COLOR),
    }
}

pub fn get_co2_gauge_percent(co2: Option<f64>) -> f32 {
    match co2 {
        Some(v) if v.is_finite() && v > 0.0 => {
            let clamped = v.clamp(CO2_GAUGE_MIN, CO2_GAUGE_MAX);
            (((clamped - CO2_GAUGE_MIN) / (CO2_GAUGE_MAX - CO2_GAUGE_MIN)) * 100.0).round() as f32
        }
        _ => 0.0,
    }
}

pub fn get_voc_display(tvoc_index: f64, nox_index: f64) -> (&'static str, Color32) {
    if !tvoc_index.is_finite() || !nox_index.is_finite() || tvoc_index <= 0.0 {
        return ("No Data", NEUTRAL_SLATE_COLOR);
    }
    if tvoc_index >= TVOC_HIGH || nox_index >= NOX_HIGH {
        return ("High", STATUS_RED);
    }
    if tvoc_index >= TVOC_ELEVATED || nox_index >= NOX_ELEVATED {
        return ("Elevated", STATUS_AMBER);
    }
    if tvoc_index >= TVOC_MODERATE || nox_index >= NOX_MODERATE {
        return ("Moderate", STATUS_SKY);
    }
    ("Normal", STATUS_GREEN)
}

pub fn format_relative_time(seconds_ago: i64) -> String {
    if seconds_ago < 0 {
        return "never".to_string();
    }
    if seconds_ago < 60 {
        return format!("{seconds_ago}s ago");
    }
    let mins = seconds_ago / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hours = mins / 60;
    format!("{hours}h ago")
}

pub fn get_connection_label(state: &str) -> &'static str {
    match state {
        "online" => "Live",
        "degraded" => "Degraded",
        "connecting" => "Connecting",
        "offline" => "Offline",
        _ => "Setup Needed",
    }
}

fn read_first_existing_font(candidates: &[PathBuf]) -> Option<Vec<u8>> {
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

/// Configures local Windows system fonts (Segoe UI + Cascadia Mono / Consolas)
/// with built-in egui font fallback for symbols. Zero network/CDN requests.
pub fn configure_fonts_and_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts_dir = windir.join("Fonts");

    let prop_candidates = [
        fonts_dir.join("segoeui.ttf"),
        fonts_dir.join("SegoeUI.ttf"),
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    ];
    if let Some(bytes) = read_first_existing_font(&prop_candidates) {
        fonts
            .font_data
            .insert("local_ui".to_owned(), Arc::new(FontData::from_owned(bytes)));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "local_ui".to_owned());
        }
    }

    let mono_candidates = [
        fonts_dir.join("CascadiaMono.ttf"),
        fonts_dir.join("consola.ttf"),
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    ];
    if let Some(bytes) = read_first_existing_font(&mono_candidates) {
        fonts.font_data.insert(
            "local_mono".to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        if let Some(list) = fonts.families.get_mut(&FontFamily::Monospace) {
            list.insert(0, "local_mono".to_owned());
        }
    }

    ctx.set_fonts(fonts);

    ctx.style_mut(|style| {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = SURFACE_TILE;
        visuals.window_stroke = Stroke::new(1.0, border_input());
        visuals.extreme_bg_color = SURFACE_TILE;
        visuals.faint_bg_color = SURFACE_TILE;
        visuals.selection.bg_fill = ACCENT_PRIMARY;
        visuals.selection.stroke = Stroke::new(1.5, ACCENT_FOCUS_CO2);
        visuals.widgets.noninteractive.bg_fill = SURFACE_TILE;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border_subtle());
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

        visuals.widgets.inactive.bg_fill = SURFACE_TILE;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border_input());
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(33, 38, 48);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 55));
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        visuals.widgets.active.bg_fill = ACCENT_PRIMARY;
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_FOCUS_CO2);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        visuals.widgets.open.bg_fill = SURFACE_TILE;
        visuals.widgets.open.bg_stroke = Stroke::new(1.5, ACCENT_FOCUS_CO2);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifts_dark_epa_colors_for_wcag_aa_contrast() {
        assert_eq!(
            ensure_readable_dark_text_color("#7F1D1D"),
            Color32::from_rgb(248, 113, 113)
        );
        assert_eq!(
            ensure_readable_dark_text_color("#8F3F97"),
            Color32::from_rgb(192, 132, 252)
        );
        assert_eq!(
            ensure_readable_dark_text_color("#10B981"),
            Color32::from_rgb(16, 185, 129)
        );
    }

    #[test]
    fn maps_voc_and_co2_thresholds() {
        assert_eq!(get_voc_display(0.0, 1.0).0, "No Data");
        assert_eq!(get_voc_display(110.0, 1.0).0, "Normal");
        assert_eq!(get_voc_display(175.0, 1.0).0, "Moderate");
        assert_eq!(get_voc_display(280.0, 1.0).0, "Elevated");
        assert_eq!(get_voc_display(420.0, 1.0).0, "High");

        assert_eq!(get_co2_category_display("optimal").0, "Good");
        assert_eq!(get_co2_category_display("critical").0, "High");
        assert_eq!(get_co2_gauge_percent(Some(400.0)), 0.0);
        assert_eq!(get_co2_gauge_percent(Some(1100.0)), 50.0);
        assert_eq!(get_co2_gauge_percent(Some(2400.0)), 100.0);
    }

    #[test]
    fn formats_relative_time() {
        assert_eq!(format_relative_time(-1), "never");
        assert_eq!(format_relative_time(12), "12s ago");
        assert_eq!(format_relative_time(125), "2m ago");
        assert_eq!(format_relative_time(7200), "2h ago");
    }
}
