#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod constants;
mod poller;
mod theme;
mod tray;
mod ui;

use eframe::egui;
use single_instance::SingleInstance;
use std::sync::Arc;

use crate::constants::{
    APP_NAME, DEFAULT_TRAY_BADGE_HEX, FLYOUT_INITIAL_HEIGHT, FLYOUT_WIDTH, SINGLE_INSTANCE_ID,
    TRAY_ICON_SIZE,
};
use crate::poller::SharedAppState;
use crate::tray::{TrayAction, TrayController, render_badge_rgba};
use crate::ui::PiGradientApp;

#[cfg(windows)]
fn spawn_windows_power_listener(wake_notify: Arc<tokio::sync::Notify>) {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
        MSG, PBT_APMRESUMEAUTOMATIC, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE,
        WM_POWERBROADCAST, WNDCLASSW, WS_OVERLAPPED,
    };
    use windows::core::PCWSTR;

    std::thread::spawn(move || unsafe {
        static WAKE_TX: std::sync::OnceLock<Arc<tokio::sync::Notify>> = std::sync::OnceLock::new();
        let _ = WAKE_TX.set(wake_notify);

        unsafe extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            if msg == WM_POWERBROADCAST
                && wparam.0 as u32 == PBT_APMRESUMEAUTOMATIC
                && let Some(notify) = WAKE_TX.get()
            {
                notify.notify_waiters();
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }

        let class_name: Vec<u16> = crate::constants::POWER_LISTENER_CLASS_NAME
            .encode_utf16()
            .collect();
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        );

        if hwnd.is_ok() {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

#[cfg(not(windows))]
fn spawn_windows_power_listener(_wake_notify: Arc<tokio::sync::Notify>) {}

fn main() -> eframe::Result<()> {
    let instance = SingleInstance::new(SINGLE_INSTANCE_ID).expect("single-instance lock creation");
    if !instance.is_single() {
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime build");

    let shared_state = SharedAppState::new();
    let wake_notify = shared_state.wake_notify.clone();
    let rt_handle = rt.handle().clone();

    let app_icon = egui::IconData {
        rgba: render_badge_rgba(DEFAULT_TRAY_BADGE_HEX),
        width: TRAY_ICON_SIZE as u32,
        height: TRAY_ICON_SIZE as u32,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_app_id(APP_NAME)
            .with_decorations(false)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_taskbar(false)
            .with_visible(false)
            .with_resizable(false)
            .with_inner_size([FLYOUT_WIDTH, FLYOUT_INITIAL_HEIGHT])
            .with_icon(Arc::new(app_icon)),
        renderer: eframe::Renderer::Glow,
        centered: false,
        persist_window: false,
        system_font_fallback: false,
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(move |cc| {
            let (tray_tx, tray_rx) = std::sync::mpsc::channel::<TrayAction>();
            let tray = TrayController::create(tray_tx, cc.egui_ctx.clone()).ok();

            poller::spawn_poller(&rt_handle, shared_state.clone(), cc.egui_ctx.clone());
            spawn_windows_power_listener(wake_notify);

            Ok(Box::new(PiGradientApp::new(
                cc,
                shared_state,
                tray,
                tray_rx,
            )))
        }),
    )
}
