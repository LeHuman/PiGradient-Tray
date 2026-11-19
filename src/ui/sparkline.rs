use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, RichText,
    Sense, Shape, Stroke, Vec2,
};

use crate::constants::{
    ACCENT_FOCUS_CO2, ACCENT_PM25, CO2_GAUGE_MIN, FLYOUT_WIDTH, MAX_SPARKLINE_POINTS,
    SURFACE_TILE, TEXT_MUTED, TEXT_SECONDARY,
};
use crate::poller::HistoryPoint;
use crate::theme::border_subtle;

use super::PiGradientApp;

impl PiGradientApp {
    pub(super) fn render_sparkline_card(
        &mut self,
        ui: &mut egui::Ui,
        history: &[HistoryPoint],
        valid_telemetry: bool,
    ) {
        let valid_points: &[HistoryPoint] = if valid_telemetry {
            let start = history.len().saturating_sub(MAX_SPARKLINE_POINTS);
            &history[start..]
        } else {
            &[]
        };
        let has_series = valid_points.len() >= 2;

        Frame::new()
            .fill(SURFACE_TILE)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, border_subtle()))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(FLYOUT_WIDTH - 44.0);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);

                // Top header row
                let active_point = if has_series {
                    let idx = self
                        .hover_history_idx
                        .filter(|&i| i < valid_points.len())
                        .unwrap_or(valid_points.len() - 1);
                    Some(&valid_points[idx])
                } else {
                    None
                };

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("1h Trend · CO₂ & PM2.5")
                            .size(11.0)
                            .color(TEXT_SECONDARY),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(pt) = active_point {
                            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                            ui.label(
                                RichText::new(format!("{:.1} µg", pt.pm02_comp))
                                    .size(10.5)
                                    .monospace()
                                    .color(ACCENT_PM25),
                            );
                            ui.label(RichText::new("/").size(10.5).color(TEXT_MUTED));
                            ui.label(
                                RichText::new(format!("{:.0} ppm", pt.rco2))
                                    .size(10.5)
                                    .monospace()
                                    .color(ACCENT_FOCUS_CO2),
                            );
                        } else {
                            ui.label(RichText::new("--").size(10.5).monospace().color(TEXT_MUTED));
                        }
                    });
                });

                // Sparkline plot area (316 x 42)
                let (plot_rect, resp) =
                    ui.allocate_exact_size(Vec2::new(FLYOUT_WIDTH - 44.0, 42.0), Sense::hover());

                if !has_series {
                    self.hover_history_idx = None;
                    let painter = ui.painter_at(plot_rect);
                    let border_pts = [
                        plot_rect.left_top(),
                        plot_rect.right_top(),
                        plot_rect.right_bottom(),
                        plot_rect.left_bottom(),
                        plot_rect.left_top(),
                    ];
                    painter.extend(Shape::dashed_line(
                        &border_pts,
                        Stroke::new(1.0, border_subtle()),
                        3.0,
                        3.0,
                    ));
                    painter.text(
                        plot_rect.center(),
                        Align2::CENTER_CENTER,
                        "No 1h history available",
                        FontId::proportional(11.0),
                        TEXT_MUTED,
                    );
                    return;
                }

                if let Some(pointer_pos) = resp.hover_pos() {
                    if plot_rect.contains(pointer_pos) {
                        let rel = ((pointer_pos.x - plot_rect.left()) / plot_rect.width())
                            .clamp(0.0, 1.0);
                        let idx = (rel * (valid_points.len() - 1) as f32).round() as usize;
                        self.hover_history_idx = Some(idx.min(valid_points.len() - 1));
                    }
                } else if !resp.hovered() {
                    self.hover_history_idx = None;
                }

                let painter = ui.painter_at(plot_rect);
                let mid_y = plot_rect.center().y;
                painter.extend(Shape::dashed_line(
                    &[
                        Pos2::new(plot_rect.left(), mid_y),
                        Pos2::new(plot_rect.right(), mid_y),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
                    2.0,
                    3.0,
                ));

                let pad_x = 2.0_f32;
                let pad_y = 4.0_f32;
                let inner_w = (plot_rect.width() - pad_x * 2.0).max(1.0);
                let inner_h = (plot_rect.height() - pad_y * 2.0).max(1.0);

                let co2_min = valid_points
                    .iter()
                    .map(|p| p.rco2)
                    .fold(CO2_GAUGE_MIN, f64::min);
                let co2_max = valid_points
                    .iter()
                    .map(|p| p.rco2)
                    .fold(co2_min + 100.0, f64::max);
                let pm_max = valid_points
                    .iter()
                    .map(|p| p.pm02_comp)
                    .fold(12.0_f64, f64::max);

                let n = valid_points.len();
                let mut pm_pts = Vec::with_capacity(n);
                let mut co2_pts = Vec::with_capacity(n);

                for (i, pt) in valid_points.iter().enumerate() {
                    let x = plot_rect.left() + pad_x + (i as f32 / (n - 1) as f32) * inner_w;
                    let pm_norm = ((pt.pm02_comp - 0.0) / pm_max.max(1.0)).clamp(0.0, 1.0) as f32;
                    let co2_norm =
                        ((pt.rco2 - co2_min) / (co2_max - co2_min).max(1.0)).clamp(0.0, 1.0) as f32;

                    pm_pts.push(Pos2::new(
                        x,
                        plot_rect.bottom() - pad_y - pm_norm * inner_h,
                    ));
                    co2_pts.push(Pos2::new(
                        x,
                        plot_rect.bottom() - pad_y - co2_norm * inner_h,
                    ));
                }

                for pair in pm_pts.windows(2) {
                    painter.line_segment([pair[0], pair[1]], Stroke::new(1.4, ACCENT_PM25));
                }
                for pair in co2_pts.windows(2) {
                    painter.line_segment([pair[0], pair[1]], Stroke::new(1.6, ACCENT_FOCUS_CO2));
                }

                if let Some(h_idx) = self.hover_history_idx.filter(|&i| i < n) {
                    let hx = co2_pts[h_idx].x;
                    painter.line_segment(
                        [
                            Pos2::new(hx, plot_rect.top()),
                            Pos2::new(hx, plot_rect.bottom()),
                        ],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 56)),
                    );
                    painter.circle_filled(pm_pts[h_idx], 2.5, ACCENT_PM25);
                    painter.circle_filled(co2_pts[h_idx], 2.8, ACCENT_FOCUS_CO2);
                }
            });
    }
}
