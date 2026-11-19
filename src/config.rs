use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::constants::{
    CONFIG_DIR_NAME, CONFIG_FILE_NAME, DEFAULT_IDLE_POLL_SECONDS, DEFAULT_TEMP_UNIT,
    DEFAULT_TRAY_ICON_STYLE,
};

fn default_temp_unit() -> String {
    DEFAULT_TEMP_UNIT.to_string()
}

fn default_tray_icon_style() -> String {
    DEFAULT_TRAY_ICON_STYLE.to_string()
}

fn default_idle_poll() -> u64 {
    DEFAULT_IDLE_POLL_SECONDS
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub gateway_url: String,
    #[serde(default = "default_temp_unit")]
    pub temp_unit: String, // "f" | "c"
    #[serde(default = "default_idle_poll")]
    pub idle_poll_seconds: u64,
    #[serde(default)]
    pub pin_flyout: bool,
    #[serde(default)]
    pub launch_at_startup: bool,
    #[serde(default = "default_tray_icon_style")]
    pub tray_icon_style: String, // "ring" | "aqi"
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            gateway_url: String::new(),
            temp_unit: default_temp_unit(),
            idle_poll_seconds: default_idle_poll(),
            pin_flyout: false,
            launch_at_startup: false,
            tray_icon_style: default_tray_icon_style(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME)
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str::<AppSettings>(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings_to_disk(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize failed: {e}"))?;
    // Write to a sibling temp file and rename it into place: a crash or power
    // loss mid-write must not leave a truncated settings.json that the loader
    // would silently replace with defaults.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Failed to write settings.json: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Failed to replace settings.json: {e}")
    })
}

pub fn normalize_gateway_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let has_explicit_scheme = trimmed
        .split_once("://")
        .map(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        })
        .unwrap_or(false);

    let with_scheme = if has_explicit_scheme {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    let parsed = reqwest::Url::parse(&with_scheme)
        .map_err(|_| "Enter a valid URL such as http://raspberrypi.local:8080".to_string())?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Gateway URL must use http:// or https://".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("Gateway URL must include a hostname or LAN IP".to_string());
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

#[cfg(windows)]
pub fn set_windows_startup(enabled: bool) -> Result<(), String> {
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
    };
    use windows::core::PCWSTR;

    let subkey: Vec<u16> = crate::constants::STARTUP_REG_SUBKEY
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = crate::constants::STARTUP_REG_VALUE_NAME
        .encode_utf16()
        .collect();

    unsafe {
        let mut hkey = HKEY::default();
        let err = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if err != ERROR_SUCCESS {
            return Err(format!("Registry open failed (code {})", err.0));
        }

        let res = if enabled {
            let exe = std::env::current_exe()
                .map_err(|e| format!("Failed to locate current executable: {e}"))?;
            let command = format!("\"{}\"\0", exe.display());
            let utf16: Vec<u16> = command.encode_utf16().collect();
            let bytes: &[u8] =
                std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), utf16.len() * 2);
            let set_err =
                RegSetValueExW(hkey, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes));
            if set_err == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(format!(
                    "Failed to write startup registry value (code {})",
                    set_err.0
                ))
            }
        } else {
            let del_err = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
            if del_err == ERROR_SUCCESS || del_err == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(format!(
                    "Failed to remove startup registry value (code {})",
                    del_err.0
                ))
            }
        };

        let _ = RegCloseKey(hkey);
        res
    }
}

#[cfg(not(windows))]
pub fn set_windows_startup(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls_without_scheme_and_strips_trailing_slash() {
        assert_eq!(
            normalize_gateway_url("192.168.1.50:8080/").unwrap(),
            "http://192.168.1.50:8080"
        );
        assert_eq!(
            normalize_gateway_url("https://air.lan/").unwrap(),
            "https://air.lan"
        );
        assert_eq!(normalize_gateway_url("   ").unwrap(), "");
    }

    #[test]
    fn rejects_non_http_schemes() {
        let err = normalize_gateway_url("ftp://raspberrypi.local").unwrap_err();
        assert!(err.contains("http:// or https://"));
    }

    #[test]
    fn parses_partial_settings_json_with_defaults() {
        let parsed: AppSettings = serde_json::from_str(r#"{"gatewayUrl":"http://pi.local:8080"}"#)
            .expect("partial JSON should deserialize cleanly");
        assert_eq!(parsed.gateway_url, "http://pi.local:8080");
        assert_eq!(parsed.temp_unit, "f");
        assert_eq!(parsed.idle_poll_seconds, 30);
        assert_eq!(parsed.tray_icon_style, "ring");
        assert!(!parsed.pin_flyout);
        assert!(!parsed.launch_at_startup);
    }
}
