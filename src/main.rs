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
    APP_NAME, DEFAULT_TRAY_BADGE_HEX, FLYOUT_INITIAL_HEIGHT, FLYOUT_OUTER_WIDTH, PARKED_POS,
    SINGLE_INSTANCE_ID, TRAY_ICON_SIZE,
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
            .with_position(PARKED_POS)
            .with_inner_size([FLYOUT_OUTER_WIDTH, FLYOUT_INITIAL_HEIGHT])
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

#[cfg(windows)]
fn hwnd_from_frame(frame: &eframe::Frame) -> Option<windows::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;

    let handle = frame.window_handle().ok()?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return None;
    };
    Some(HWND(win32.hwnd.get() as *mut core::ffi::c_void))
}

/// DWM draws a rectangular drop shadow plus a 1px border around every window
/// that has frame styles, even when `decorations(false)` hides the title bar.
/// Furthermore, `egui-winit` passes `with_undecorated_shadow(true)` on Windows
/// (ignoring `with_has_shadow(false)`), which triggers winit's `WM_NCCALCSIZE`
/// 1px top/bottom inset hack (`MARKER_UNDECORATED_SHADOW`). With DWM NC
/// rendering disabled, that 1px strip renders as a dark line across the top and
/// shifts the client surface down by 1px.
///
/// Subclassing the HWND once to return `0` for `WM_NCCALCSIZE` (leaving the
/// full window rect as client area) and `1` for `WM_ERASEBKGND` (preventing
/// white GDI background flashes on move/resize) eliminates both defects cleanly.
#[cfg(windows)]
fn ensure_borderless_window_policy(frame: &eframe::Frame) {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_CLOAK, DWMWA_NCRENDERING_POLICY, DWMWA_TRANSITIONS_FORCEDISABLED,
        DwmSetWindowAttribute,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GWLP_WNDPROC, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WM_ERASEBKGND,
        WM_NCCALCSIZE, WNDPROC,
    };

    static HOOKED_HWND: AtomicIsize = AtomicIsize::new(0);
    static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);

    unsafe extern "system" fn borderless_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_NCCALCSIZE && wparam.0 != 0 {
            return LRESULT(0);
        }
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        let orig = ORIG_WNDPROC.load(Ordering::Acquire);
        if orig != 0 {
            let proc: WNDPROC = unsafe { std::mem::transmute::<isize, WNDPROC>(orig) };
            unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) }
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    let Some(hwnd) = hwnd_from_frame(frame) else {
        return;
    };
    let hwnd_val = hwnd.0 as isize;

    if HOOKED_HWND
        .compare_exchange(0, hwnd_val, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        unsafe {
            let prev = SetWindowLongPtrW(
                hwnd,
                GWLP_WNDPROC,
                borderless_wnd_proc as *const () as usize as isize,
            );
            ORIG_WNDPROC.store(prev, Ordering::Release);

            let disable_transitions: i32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                &disable_transitions as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );

            let cloaked: i32 = 1;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CLOAK,
                &cloaked as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );

            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    // Re-applied on every call instead of once: DWM drops the non-client
    // policy when the window is moved or resized, which is exactly what the
    // flyout does on each show. The call is cheap.
    let policy: i32 = DWMNCRP_DISABLED.0;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

#[cfg(not(windows))]
fn ensure_borderless_window_policy(_frame: &eframe::Frame) {}

/// Controls DWM cloaking (`DWMWA_CLOAK`) so a mapped window can settle its
/// size and swap buffers offscreen before DWM composites it onto the desktop.
#[cfg(windows)]
pub(crate) fn set_window_cloaked(frame: &eframe::Frame, cloaked: bool) {
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};

    let Some(hwnd) = hwnd_from_frame(frame) else {
        return;
    };
    let val: i32 = i32::from(cloaked);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK,
            &val as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

#[cfg(not(windows))]
pub(crate) fn set_window_cloaked(_frame: &eframe::Frame, _cloaked: bool) {}
