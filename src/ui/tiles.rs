use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Vec2,
};

use crate::config::AppSettings;
use crate::constants::{
    ACCENT_PM25, FLYOUT_WIDTH, NEUTRAL_SLATE_HEX, SURFACE_TILE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};
use crate::poller::{LivePayload, TelemetrySnapshot, WidgetPayload};
use crate::theme::{
    border_subtle, ensure_readable_dark_text_color, get_co2_category_display,
    get_co2_gauge_percent, get_voc_display, hover_overlay,
};

pub(super) fn render_kpi_grid(
    ui: &mut egui::Ui,
    snapshot: &TelemetrySnapshot,
    settings: &AppSettings,
    valid: bool,
) {
    let live: Option<&LivePayload> = if valid { snapshot.live.as_ref() } else { None };
    let widget: Option<&WidgetPayload> = if valid {
        snapshot.widget.as_ref()
    } else {
        None
    };

    let tile_w = (FLYOUT_WIDTH - 24.0 - 8.0) * 0.5; // 164.0 px exact

    // Row 1: PM2.5 + CO2
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

        // Tile 1: PM2.5 / AQI
        let aqi_val = live
            .map(|l| l.processed.aqi_pm25.round())
            .or_else(|| widget.map(|w| w.aqi.round()));
        let aqi_label = live
            .map(|l| l.processed.aqi_category.as_str())
            .or_else(|| widget.map(|w| w.aqi_desc.as_str()))
            .filter(|s| !s.is_empty())
            .unwrap_or("No Data");
        let aqi_hex = live
            .map(|l| l.processed.aqi_color.as_str())
            .or_else(|| widget.map(|w| w.aqi_color.as_str()))
            .unwrap_or(NEUTRAL_SLATE_HEX);
        let aqi_text_color = if valid {
            ensure_readable_dark_text_color(aqi_hex)
        } else {
            TEXT_SECONDARY
        };
        let pm_sub = if let Some(l) = live {
            format!(
                "{:.1} µg · NowCast {:.0}",
                l.raw.pm02_comp,
                l.processed.nowcast_aqi.round()
            )
        } else if valid {
            "Idle summary".to_string()
        } else {
            "Awaiting gateway".to_string()
        };

        tile_frame(ui, tile_w, 88.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("PM2.5").size(11.0).color(TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(if valid { aqi_label } else { "No Data" })
                                .size(11.0)
                                .strong()
                                .color(aqi_text_color),
                        )
                        .truncate(),
                    );
                });
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(5.0, 0.0);
                let val_str = match (valid, aqi_val) {
                    (true, Some(v)) => format!("{v:.0}"),
                    _ => "--".to_string(),
                };
                ui.label(
                    RichText::new(val_str)
                        .size(22.0)
                        .strong()
                        .monospace()
                        .color(Color32::WHITE),
                );
                ui.label(RichText::new("AQI").size(11.0).color(TEXT_MUTED));
            });

            ui.add(
                egui::Label::new(
                    RichText::new(pm_sub)
                        .size(10.0)
                        .monospace()
                        .color(TEXT_MUTED),
                )
                .truncate(),
            );
        });

        // Tile 2: CO2
        let co2_val = live
            .map(|l| l.raw.rco2.round())
            .or_else(|| widget.map(|w| w.co2.round()));
        let (co2_label, co2_color) = if let Some(l) = live {
            get_co2_category_display(&l.processed.co2_category)
        } else if valid {
            ("Live", ACCENT_PM25)
        } else {
            ("No Data", TEXT_SECONDARY)
        };
        let co2_pct = if valid {
            get_co2_gauge_percent(co2_val)
        } else {
            0.0
        };

        tile_frame(ui, tile_w, 88.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("CO₂").size(11.0).color(TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(if valid { co2_label } else { "No Data" })
                                .size(11.0)
                                .strong()
                                .color(if valid { co2_color } else { TEXT_SECONDARY }),
                        )
                        .truncate(),
                    );
                });
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                let val_str = match (valid, co2_val) {
                    (true, Some(v)) => format!("{v:.0}"),
                    _ => "--".to_string(),
                };
                ui.label(
                    RichText::new(val_str)
                        .size(22.0)
                        .strong()
                        .monospace()
                        .color(Color32::WHITE),
                );
                ui.label(RichText::new("ppm").size(11.0).color(TEXT_MUTED));
            });

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 3.0);
                let (bar_rect, _) =
                    ui.allocate_exact_size(Vec2::new(tile_w - 20.0, 6.0), Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(bar_rect, CornerRadius::same(3), hover_overlay());
                if valid {
                    let fill_frac = (co2_pct.max(6.0) / 100.0).clamp(0.06, 1.0);
                    let fill_rect = Rect::from_min_size(
                        bar_rect.min,
                        Vec2::new(bar_rect.width() * fill_frac, bar_rect.height()),
                    );
                    painter.rect_filled(fill_rect, CornerRadius::same(3), co2_color);
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("400").size(9.5).monospace().color(TEXT_MUTED));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("1800")
                                .size(9.5)
                                .monospace()
                                .color(TEXT_MUTED),
                        );
                    });
                });
            });
        });
    });

    // Row 2: Climate + VOCs
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

        // Tile 3: Climate
        let temp_str = if let Some(l) = live {
            if settings.temp_unit == "f" {
                format!("{:.1}°F", l.processed.temp_f)
            } else {
                format!("{:.1}°C", l.processed.temp_c)
            }
        } else if let Some(w) = widget {
            if settings.temp_unit == "f" {
                w.temp.clone()
            } else {
                w.temp_c.clone()
            }
        } else {
            "--".to_string()
        };

        let rh_str = if let Some(l) = live {
            format!("{:.0}%", l.processed.humidity.round())
        } else if let Some(w) = widget {
            w.humidity.clone()
        } else {
            "--".to_string()
        };

        let hi_str = if let Some(l) = live {
            if settings.temp_unit == "f" {
                format!("HI {:.0}°", l.processed.heat_index_f)
            } else {
                format!("HI {:.0}°", l.processed.heat_index_c)
            }
        } else {
            "Indoor".to_string()
        };

        let dew_str = if let Some(l) = live {
            if settings.temp_unit == "f" {
                format!("Dew {:.1}°F", l.processed.dew_point_f)
            } else {
                format!("Dew {:.1}°C", l.processed.dew_point_c)
            }
        } else if valid {
            "Compensated".to_string()
        } else {
            "Temp & humidity".to_string()
        };

        tile_frame(ui, tile_w, 82.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Climate").size(11.0).color(TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(hi_str)
                            .size(11.0)
                            .monospace()
                            .color(TEXT_MUTED),
                    );
                });
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(5.0, 0.0);
                ui.label(
                    RichText::new(temp_str)
                        .size(19.0)
                        .strong()
                        .monospace()
                        .color(Color32::WHITE),
                );
                ui.label(RichText::new("·").size(12.0).color(TEXT_MUTED));
                ui.label(
                    RichText::new(rh_str)
                        .size(16.0)
                        .monospace()
                        .color(TEXT_PRIMARY),
                );
            });

            ui.add(
                egui::Label::new(
                    RichText::new(dew_str)
                        .size(10.5)
                        .monospace()
                        .color(TEXT_MUTED),
                )
                .truncate(),
            );
        });

        // Tile 4: VOCs
        let (voc_label, voc_color) = if let Some(l) = live {
            get_voc_display(l.raw.tvoc_index, l.raw.nox_index)
        } else if valid {
            ("Idle", TEXT_SECONDARY)
        } else {
            ("--", Color32::WHITE)
        };
        let voc_sub = if let Some(l) = live {
            format!(
                "TVOC {:.0} · NOx {:.0}",
                l.raw.tvoc_index.round(),
                l.raw.nox_index.round()
            )
        } else if valid {
            "Live stream on focus".to_string()
        } else {
            "VOC & NOx index".to_string()
        };

        tile_frame(ui, tile_w, 82.0, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("VOCs").size(11.0).color(TEXT_SECONDARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Index").size(10.5).color(TEXT_MUTED));
                });
            });

            ui.label(
                RichText::new(if valid { voc_label } else { "--" })
                    .size(18.0)
                    .strong()
                    .color(if valid { voc_color } else { Color32::WHITE }),
            );

            ui.add(
                egui::Label::new(
                    RichText::new(voc_sub)
                        .size(10.5)
                        .monospace()
                        .color(TEXT_MUTED),
                )
                .truncate(),
            );
        });
    });
}

fn tile_frame(
    ui: &mut egui::Ui,
    width: f32,
    min_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let mut frame = Frame::new()
        .fill(SURFACE_TILE)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, border_subtle()))
        .inner_margin(Margin::same(10));
    frame.stroke = Stroke::new(1.0, border_subtle());
    frame.show(ui, |ui| {
        ui.set_width(width - 20.0);
        ui.set_min_height(min_height - 20.0);
        ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 5.0);
            add_contents(ui);
        });
    });
}
