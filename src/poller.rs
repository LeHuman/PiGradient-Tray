use chrono::Utc;
use eframe::egui;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Notify;

use crate::config::{AppSettings, load_settings};
use crate::constants::{
    BACKOFF_INITIAL_SECS, BACKOFF_MAX_SECS, BACKOFF_MED_SECS, HISTORY_INTERVAL_SECS,
    HISTORY_STALE_MS, HISTORY_WINDOW_SECS, HTTP_TIMEOUT_SECS, MAX_IDLE_POLL_SECONDS,
    MIN_IDLE_POLL_SECONDS, NEUTRAL_SLATE_HEX, SSE_CONNECT_TIMEOUT_SECS, USER_AGENT,
};
use crate::tray::format_tooltip_text;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetPayload {
    pub aqi: f64,
    pub aqi_desc: String,
    pub aqi_color: String,
    pub co2: f64,
    pub temp: String,
    pub temp_c: String,
    pub humidity: String,
    pub updated: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceInfo {
    pub serialno: String,
    pub firmware: String,
    pub model: String,
    pub wifi_rssi: i32,
    pub boot_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RawTelemetry {
    pub pm01: f64,
    pub pm02: f64,
    pub pm10: f64,
    pub pm02_comp: f64,
    pub rco2: f64,
    pub atmp: f64,
    pub atmp_comp: f64,
    pub rhum: f64,
    pub rhum_comp: f64,
    pub tvoc_index: f64,
    pub tvoc_raw: f64,
    pub nox_index: f64,
    pub nox_raw: f64,
    pub boot_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessedTelemetry {
    pub aqi_pm25: f64,
    pub aqi_category: String,
    pub aqi_color: String,
    pub nowcast_aqi: f64,
    pub co2_level: String,
    pub co2_category: String,
    pub temp_f: f64,
    pub temp_c: f64,
    pub humidity: f64,
    pub dew_point_c: f64,
    pub dew_point_f: f64,
    pub heat_index_c: f64,
    pub heat_index_f: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LivePayload {
    pub status: String,
    pub is_simulated: bool,
    pub error_message: Option<String>,
    pub last_seen_seconds_ago: i64,
    pub timestamp: i64,
    pub latency_ms: u64,
    pub device: DeviceInfo,
    pub raw: RawTelemetry,
    pub processed: ProcessedTelemetry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryPoint {
    pub timestamp: i64,
    pub pm01: f64,
    pub pm02_comp: f64,
    pub pm10: f64,
    pub rco2: f64,
    pub atmp_comp: f64,
    pub rhum_comp: f64,
    pub tvoc_index: f64,
    pub nox_index: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    pub connection_state: String,
    pub live: Option<LivePayload>,
    pub widget: Option<WidgetPayload>,
    pub history: Vec<HistoryPoint>,
    pub last_error: Option<String>,
    pub last_updated_at_ms: i64,
}

impl Default for TelemetrySnapshot {
    fn default() -> Self {
        Self {
            connection_state: "unconfigured".to_string(),
            live: None,
            widget: None,
            history: Vec::new(),
            last_error: None,
            last_updated_at_ms: 0,
        }
    }
}

impl TelemetrySnapshot {
    pub fn has_valid_telemetry(&self) -> bool {
        if self.connection_state == "unconfigured" || self.connection_state == "offline" {
            return false;
        }
        if let Some(live) = &self.live
            && live.timestamp > 0
        {
            return true;
        }
        if let Some(widget) = &self.widget
            && widget.updated != "--"
            && widget.aqi_desc != "No Data"
        {
            return true;
        }
        false
    }

    pub fn select_badge_color(&self) -> String {
        if !self.has_valid_telemetry() {
            return NEUTRAL_SLATE_HEX.to_string();
        }
        if let Some(live) = &self.live
            && live.timestamp > 0
            && !live.processed.aqi_color.trim().is_empty()
        {
            return live.processed.aqi_color.clone();
        }
        if let Some(widget) = &self.widget
            && !widget.aqi_color.trim().is_empty()
        {
            return widget.aqi_color.clone();
        }
        NEUTRAL_SLATE_HEX.to_string()
    }

    pub fn tray_visual_inputs(&self, settings: &AppSettings) -> (String, Option<String>, String) {
        let badge = self.select_badge_color();
        let tooltip = if let Some(live) = &self.live {
            if live.timestamp > 0 {
                format_tooltip_text(
                    &self.connection_state,
                    Some(live.processed.aqi_pm25),
                    Some(&live.processed.aqi_category),
                    Some(live.raw.rco2),
                    Some(live.processed.temp_f),
                    Some(live.processed.temp_c),
                    Some(live.processed.humidity),
                    Some(live.last_seen_seconds_ago),
                    settings,
                )
            } else {
                format_tooltip_text(
                    &self.connection_state,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(live.last_seen_seconds_ago),
                    settings,
                )
            }
        } else if let Some(widget) = &self.widget {
            if widget.updated != "--" && widget.aqi_desc != "No Data" {
                let tf = widget.temp.trim_end_matches("°F").parse::<f64>().ok();
                let tc = widget.temp_c.trim_end_matches("°C").parse::<f64>().ok();
                let rh = widget.humidity.trim_end_matches('%').parse::<f64>().ok();
                format_tooltip_text(
                    &self.connection_state,
                    Some(widget.aqi),
                    Some(&widget.aqi_desc),
                    Some(widget.co2),
                    tf,
                    tc,
                    rh,
                    None,
                    settings,
                )
            } else {
                format_tooltip_text(
                    &self.connection_state,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    settings,
                )
            }
        } else {
            format_tooltip_text(
                &self.connection_state,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                settings,
            )
        };

        let aqi_label = if !self.has_valid_telemetry() {
            None
        } else if let Some(live) = &self.live {
            if live.timestamp > 0 {
                Some(format!("{:.0}", live.processed.aqi_pm25.clamp(0.0, 999.0)))
            } else {
                None
            }
        } else if let Some(widget) = &self.widget {
            if widget.updated != "--" && widget.aqi_desc != "No Data" {
                Some(format!("{:.0}", widget.aqi.clamp(0.0, 999.0)))
            } else {
                None
            }
        } else {
            None
        };

        (badge, aqi_label, tooltip)
    }
}

pub struct SharedAppState {
    pub settings: RwLock<AppSettings>,
    pub snapshot: RwLock<TelemetrySnapshot>,
    pub flyout_visible: AtomicBool,
    pub wake_notify: Arc<Notify>,
    pub http: reqwest::Client,
    pub sse_http: reqwest::Client,
    pub last_history_fetch_ms: AtomicI64,
}

impl SharedAppState {
    pub fn new() -> Arc<Self> {
        let initial_settings = load_settings();
        let mut initial_snapshot = TelemetrySnapshot::default();
        if !initial_settings.gateway_url.trim().is_empty() {
            initial_snapshot.connection_state = "connecting".to_string();
        }
        Arc::new(Self {
            settings: RwLock::new(initial_settings),
            snapshot: RwLock::new(initial_snapshot),
            flyout_visible: AtomicBool::new(false),
            wake_notify: Arc::new(Notify::new()),
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
                .build()
                .expect("HTTP client build"),
            sse_http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(SSE_CONNECT_TIMEOUT_SECS))
                .build()
                .expect("SSE HTTP client build"),
            last_history_fetch_ms: AtomicI64::new(0),
        })
    }

    pub fn read_settings(&self) -> AppSettings {
        self.settings.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn read_snapshot(&self) -> TelemetrySnapshot {
        self.snapshot.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn set_flyout_visible(&self, visible: bool) {
        self.flyout_visible.store(visible, Ordering::SeqCst);
        self.wake_notify.notify_waiters();
    }

    pub fn trigger_refresh(&self) {
        self.wake_notify.notify_waiters();
    }
}

async fn fetch_history_if_stale(state: &SharedAppState, base_url: &str) {
    let now_ms = Utc::now().timestamp_millis();
    let prev_ms = state.last_history_fetch_ms.load(Ordering::Relaxed);
    let has_history = state
        .snapshot
        .read()
        .map(|s| !s.history.is_empty())
        .unwrap_or(false);

    if has_history && now_ms - prev_ms < HISTORY_STALE_MS {
        return;
    }

    let now = Utc::now().timestamp();
    let start = now - HISTORY_WINDOW_SECS;
    let history_url = format!(
        "{base_url}/api/v1/history?start={start}&end={now}&interval={HISTORY_INTERVAL_SECS}"
    );
    if let Ok(resp) = state.http.get(&history_url).send().await
        && resp.status().is_success()
        && let Ok(points) = resp.json::<Vec<HistoryPoint>>().await
    {
        if let Ok(mut snap) = state.snapshot.write() {
            snap.history = points;
        }
        state.last_history_fetch_ms.store(now_ms, Ordering::Relaxed);
    }
}

pub fn spawn_poller(
    rt_handle: &tokio::runtime::Handle,
    state: Arc<SharedAppState>,
    ctx: egui::Context,
) {
    rt_handle.spawn(async move {
        let mut backoff_secs = BACKOFF_INITIAL_SECS;

        loop {
            let settings = state.read_settings();
            let base_url = settings.gateway_url.trim().trim_end_matches('/').to_string();

            if base_url.is_empty() {
                if let Ok(mut snap) = state.snapshot.write() {
                    snap.connection_state = "unconfigured".to_string();
                    snap.live = None;
                    snap.widget = None;
                    snap.history.clear();
                    snap.last_error = None;
                }
                ctx.request_repaint();
                state.wake_notify.notified().await;
                continue;
            }

            if state.flyout_visible.load(Ordering::SeqCst) {
                fetch_history_if_stale(&state, &base_url).await;
                ctx.request_repaint();

                let stream_url = format!("{base_url}/api/v1/stream");
                match state.sse_http.get(&stream_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        backoff_secs = BACKOFF_INITIAL_SECS;
                        let mut stream = resp.bytes_stream();
                        let mut buffer = String::new();

                        while state.flyout_visible.load(Ordering::SeqCst) {
                            tokio::select! {
                                _ = state.wake_notify.notified() => {
                                    if !state.flyout_visible.load(Ordering::SeqCst) {
                                        break;
                                    }
                                    // Refresh URL / history if user just edited settings
                                    let latest_url = state.read_settings().gateway_url.trim().trim_end_matches('/').to_string();
                                    if latest_url != base_url {
                                        break;
                                    }
                                }
                                chunk = stream.next() => {
                                    match chunk {
                                        Some(Ok(bytes)) => {
                                            buffer.push_str(&String::from_utf8_lossy(&bytes));
                                            while let Some(pos) = buffer.find("\n\n") {
                                                let event_block = buffer[..pos].to_string();
                                                buffer.drain(..pos + 2);
                                                for line in event_block.lines() {
                                                    if let Some(json_str) = line.strip_prefix("data:")
                                                        && let Ok(live) = serde_json::from_str::<LivePayload>(json_str.trim()) {
                                                            if let Ok(mut snap) = state.snapshot.write() {
                                                                snap.connection_state = live.status.clone();
                                                                snap.last_error = live.error_message.clone();
                                                                snap.last_updated_at_ms = Utc::now().timestamp_millis();
                                                                snap.live = Some(live);
                                                            }
                                                            ctx.request_repaint();
                                                        }
                                                }
                                            }
                                        }
                                        _ => break,
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut snap) = state.snapshot.write() {
                            snap.connection_state = "offline".to_string();
                            snap.last_error = Some(format!("Gateway unreachable: {e}"));
                        }
                        ctx.request_repaint();

                        let sleep_dur = Duration::from_secs(backoff_secs);
                        backoff_secs = if backoff_secs == BACKOFF_INITIAL_SECS {
                            BACKOFF_MED_SECS
                        } else {
                            BACKOFF_MAX_SECS
                        };
                        tokio::select! {
                            _ = tokio::time::sleep(sleep_dur) => {}
                            _ = state.wake_notify.notified() => {}
                        }
                    }
                    Ok(resp) => {
                        if let Ok(mut snap) = state.snapshot.write() {
                            snap.connection_state = "offline".to_string();
                            snap.last_error = Some(format!("Gateway HTTP {}", resp.status()));
                        }
                        ctx.request_repaint();

                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                            _ = state.wake_notify.notified() => {}
                        }
                    }
                }
            } else {
                // Idle tray mode: single tiny /api/v1/widget poll
                let widget_url = format!("{base_url}/api/v1/widget");
                match state.http.get(&widget_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(widget) = resp.json::<WidgetPayload>().await {
                            backoff_secs = BACKOFF_INITIAL_SECS;
                            if let Ok(mut snap) = state.snapshot.write() {
                                snap.connection_state = "online".to_string();
                                snap.widget = Some(widget);
                                snap.last_error = None;
                                snap.last_updated_at_ms = Utc::now().timestamp_millis();
                            }
                        }
                    }
                    Ok(resp) => {
                        if let Ok(mut snap) = state.snapshot.write() {
                            snap.connection_state = "offline".to_string();
                            snap.last_error = Some(format!("Gateway HTTP {}", resp.status()));
                        }
                    }
                    Err(e) => {
                        if let Ok(mut snap) = state.snapshot.write() {
                            snap.connection_state = "offline".to_string();
                            snap.last_error = Some(format!("Gateway unreachable: {e}"));
                        }
                    }
                }

                // Trigger App::logic() once to refresh tray icon/tooltip if changed
                ctx.request_repaint();

                let poll_secs = settings
                    .idle_poll_seconds
                    .clamp(MIN_IDLE_POLL_SECONDS, MAX_IDLE_POLL_SECONDS);
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(poll_secs)) => {}
                    _ = state.wake_notify.notified() => {}
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_partial_gateway_payload_without_crashing() {
        let json = r##"{
            "status": "online",
            "timestamp": 1700000000,
            "device": { "model": "ONE_INDOOR" },
            "raw": { "rco2": 620 },
            "processed": { "aqi_pm25": 18, "aqi_color": "#10B981" }
        }"##;
        let live: LivePayload = serde_json::from_str(json).expect("partial payload parses");
        assert_eq!(live.device.model, "ONE_INDOOR");
        assert_eq!(live.device.firmware, "");
        assert_eq!(live.raw.pm02_comp, 0.0);
        assert_eq!(live.processed.aqi_pm25, 18.0);
    }

    #[test]
    fn enforces_neutral_slate_when_timestamp_zero_or_offline() {
        let mut snap = TelemetrySnapshot {
            connection_state: "online".to_string(),
            live: Some(LivePayload {
                timestamp: 0,
                processed: ProcessedTelemetry {
                    aqi_color: "#10B981".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!snap.has_valid_telemetry());
        assert_eq!(snap.select_badge_color(), NEUTRAL_SLATE_HEX);

        snap.live.as_mut().unwrap().timestamp = 1700000000;
        assert!(snap.has_valid_telemetry());
        assert_eq!(snap.select_badge_color(), "#10B981");

        snap.connection_state = "offline".to_string();
        assert!(!snap.has_valid_telemetry());
        assert_eq!(snap.select_badge_color(), NEUTRAL_SLATE_HEX);
    }
}
