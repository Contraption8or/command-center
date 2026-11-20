//! Windows desktop notifications (toasts) for reminders.
//!
//! Windows labels a toast with the app that sent it. An unpackaged app does that by
//! registering an AppUserModelID under `HKCU\Software\Classes\AppUserModelId` with a
//! display name and icon; [`setup`] does this once per start (it's cheap and keeps the icon
//! path current if the data folder moves). The taskbar identity is left alone, so pinned
//! shortcuts keep working.

#[cfg(windows)]
pub const APP_ID: &str = if cfg!(debug_assertions) { "Contraption8or.CommandCenter.Dev" } else { "Contraption8or.CommandCenter" };

/// Register the app's name and icon for notifications. Failures only mean toasts show a
/// generic name, so they're ignored.
pub fn setup(data_dir: &std::path::Path) {
    #[cfg(windows)]
    {
        let icon = data_dir.join("app-icon.png");
        let _ = std::fs::write(&icon, png(&crate::icon_art::icon_rgba(128), 128));
        let name = if cfg!(debug_assertions) { "Command Center (dev)" } else { "Command Center" };
        registry::set_app_id(APP_ID, name, &icon);
    }
    #[cfg(not(windows))]
    let _ = data_dir;
}

/// Show a notification. Clicking it brings Command Center to the front.
pub fn show(title: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::{Sound, Toast};
        Toast::new(APP_ID)
            .title(title)
            .text1(body)
            .sound(Some(Sound::Reminder))
            .on_activated(|_| {
                focus_main_window();
                Ok(())
            })
            .show()
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (title, body);
        Err("notifications are only available on Windows".into())
    }
}

#[cfg(windows)]
fn focus_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE};
    let title: Vec<u16> = (if cfg!(debug_assertions) { "Command Center (dev)" } else { "Command Center" })
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: plain Win32 calls on a window handle we just looked up.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(windows)]
mod registry {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn set_app_id(app_id: &str, name: &str, icon: &std::path::Path) {
        let path = wide(&format!(r"Software\Classes\AppUserModelId\{app_id}"));
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: valid null-terminated strings; the key is closed below.
        unsafe {
            if RegCreateKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, std::ptr::null(), &mut key, std::ptr::null_mut()) != 0 {
                return;
            }
            for (value, data) in [("DisplayName", name.to_string()), ("IconUri", icon.to_string_lossy().into_owned())] {
                let v = wide(value);
                let d = wide(&data);
                RegSetValueExW(key, v.as_ptr(), 0, REG_SZ, d.as_ptr() as *const u8, (d.len() * 2) as u32);
            }
            RegCloseKey(key);
        }
    }
}

/// Minimal PNG encoder (stored, uncompressed deflate) for the notification icon.
#[cfg(windows)]
fn png(rgba: &[u8], size: u32) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    // Raw scanlines: filter byte 0 + RGBA row.
    let row = (size * 4) as usize;
    let mut raw = Vec::with_capacity((row + 1) * size as usize);
    for y in 0..size as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }
    // zlib stream with stored blocks.
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65_535).enumerate() {
        let last = (i + 1) * 65_535 >= raw.len();
        z.push(last as u8);
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Whether Windows notifications are switched off for the whole PC
/// (Settings → System → Notifications), in which case toasts are dropped silently.
pub fn blocked_by_windows() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
        let key: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\PushNotifications".encode_utf16().chain([0]).collect();
        let name: Vec<u16> = "ToastEnabled".encode_utf16().chain([0]).collect();
        let mut value: u32 = 1;
        let mut size = 4u32;
        // SAFETY: reading a DWORD into a 4-byte buffer.
        let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut value as *mut u32 as *mut _, &mut size) };
        rc == 0 && value == 0
    }
    #[cfg(not(windows))]
    false
}

/// Flash the taskbar button until the window is focused (a reminder fallback that works even
/// with notifications off).
pub fn flash_taskbar() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, FlashWindowEx, GetForegroundWindow, FLASHWINFO, FLASHW_ALL, FLASHW_TIMERNOFG};
        let title: Vec<u16> = (if cfg!(debug_assertions) { "Command Center (dev)" } else { "Command Center" }).encode_utf16().chain([0]).collect();
        // SAFETY: plain Win32 calls on our own window.
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if hwnd.is_null() || GetForegroundWindow() == hwnd {
                return;
            }
            let info = FLASHWINFO { cbSize: std::mem::size_of::<FLASHWINFO>() as u32, hwnd, dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG, uCount: 0, dwTimeout: 0 };
            FlashWindowEx(&info);
        }
    }
}
