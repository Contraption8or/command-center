//! Window size/position persistence and the generated app icon.

use crate::store::use_store;
use cc_core::Db;
use dioxus::desktop::tao::dpi::{PhysicalPosition, PhysicalSize};
use dioxus::desktop::tao::event::{Event, WindowEvent};
use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{use_window, use_wry_event_handler, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const KEY: &str = "window";

/// Saved as "x,y,width,height,maximized" in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Saved {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    maximized: bool,
}

impl Saved {
    fn parse(s: &str) -> Option<Self> {
        let p: Vec<&str> = s.split(',').collect();
        if p.len() != 5 {
            return None;
        }
        let saved = Saved {
            x: p[0].parse().ok()?,
            y: p[1].parse().ok()?,
            w: p[2].parse().ok()?,
            h: p[3].parse().ok()?,
            maximized: p[4] == "1",
        };
        (saved.w >= 400 && saved.h >= 300).then_some(saved)
    }
    fn encode(&self) -> String {
        format!("{},{},{},{},{}", self.x, self.y, self.w, self.h, self.maximized as u8)
    }
}

/// Window builder restored from the last session (or a sensible default).
pub fn builder(db: &Db) -> WindowBuilder {
    let base = WindowBuilder::new()
        .with_title(if cfg!(debug_assertions) { "Command Center (dev)" } else { "Command Center" })
        .with_min_inner_size(LogicalSize::new(820.0, 560.0))
        .with_window_icon(Some(icon()));
    match db.setting(KEY).as_deref().and_then(Saved::parse) {
        Some(s) => base
            .with_inner_size(PhysicalSize::new(s.w, s.h))
            .with_position(PhysicalPosition::new(s.x, s.y))
            .with_maximized(s.maximized),
        None => base.with_inner_size(LogicalSize::new(1440.0, 900.0)),
    }
}

/// Windows keeps the "normal" (restored) rectangle separately from the maximized state;
/// reading and writing that placement avoids saving a maximized size as the normal one.
#[cfg(windows)]
mod placement {
    use super::Saved;
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    use dioxus::desktop::tao::window::Window;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowPlacement, SetWindowPlacement, WINDOWPLACEMENT};

    const SW_SHOWNORMAL: u32 = 1;
    const SW_SHOWMINIMIZED: u32 = 2;
    const SW_SHOWMAXIMIZED: u32 = 3;
    const WPF_RESTORETOMAXIMIZED: u32 = 2;

    fn empty() -> WINDOWPLACEMENT {
        WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            flags: 0,
            showCmd: 0,
            ptMinPosition: Default::default(),
            ptMaxPosition: Default::default(),
            rcNormalPosition: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        }
    }

    pub fn read(win: &Window) -> Option<Saved> {
        let mut p = empty();
        // SAFETY: valid HWND owned by this process; `p` is a properly sized WINDOWPLACEMENT.
        let ok = unsafe { GetWindowPlacement(win.hwnd() as _, &mut p) } != 0;
        if !ok {
            return None;
        }
        let r = p.rcNormalPosition;
        let maximized = p.showCmd == SW_SHOWMAXIMIZED || (p.showCmd == SW_SHOWMINIMIZED && p.flags & WPF_RESTORETOMAXIMIZED != 0);
        Some(Saved { x: r.left, y: r.top, w: (r.right - r.left) as u32, h: (r.bottom - r.top) as u32, maximized })
    }

    pub fn apply(win: &Window, s: &Saved) {
        let mut p = empty();
        p.showCmd = if s.maximized { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL };
        p.rcNormalPosition = RECT { left: s.x, top: s.y, right: s.x + s.w as i32, bottom: s.y + s.h as i32 };
        // SAFETY: as above.
        unsafe { SetWindowPlacement(win.hwnd() as _, &p) };
    }
}

/// Save size/position as the window moves (at most every 400 ms, plus once on close),
/// and pull the window back on screen if its saved monitor is gone.
pub fn use_window_persistence() {
    let store = use_store();
    let win = use_window();
    let last = use_hook(|| Rc::new(Cell::new(Instant::now() - Duration::from_secs(1))));

    // Re-apply the exact saved placement (the builder only approximates it).
    #[cfg(windows)]
    use_hook({
        let win = win.clone();
        let saved = store.setting(KEY).as_deref().and_then(Saved::parse);
        move || {
            if let Some(s) = saved {
                placement::apply(&win, &s);
            }
        }
    });

    use_hook({
        let win = win.clone();
        move || {
            let pos = win.outer_position().ok();
            let size = win.outer_size();
            let visible = pos.is_some_and(|p| {
                win.available_monitors().any(|m| {
                    let (mp, ms) = (m.position(), m.size());
                    // At least the top-left 100px of the title bar must land on a monitor.
                    p.x + 100 > mp.x && p.x < mp.x + ms.width as i32 && p.y >= mp.y - 10 && p.y < mp.y + ms.height as i32 - 50
                })
            });
            if !visible {
                if let Some(m) = win.primary_monitor().or_else(|| win.available_monitors().next()) {
                    let (mp, ms) = (m.position(), m.size());
                    let x = mp.x + (ms.width as i32 - size.width as i32).max(0) / 2;
                    let y = mp.y + (ms.height as i32 - size.height as i32).max(0) / 2;
                    win.set_outer_position(PhysicalPosition::new(x, y));
                }
            }
        }
    });

    use_wry_event_handler(move |event, _| {
        let Event::WindowEvent { event, .. } = event else { return };
        let closing = matches!(event, WindowEvent::CloseRequested);
        if !closing && !matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
            return;
        }
        if !closing && last.get().elapsed() < Duration::from_millis(400) {
            return;
        }
        last.set(Instant::now());
        #[cfg(windows)]
        {
            if let Some(s) = placement::read(&win) {
                store.set_setting(KEY, &s.encode());
            }
            if closing {
                let _ = store.peek(|db| db.checkpoint());
            }
            return;
        }
        #[allow(unreachable_code)]
        if win.is_minimized() {
            return;
        }
        let maximized = win.is_maximized();
        let mut s = store.setting(KEY).as_deref().and_then(Saved::parse).unwrap_or(Saved { x: 100, y: 100, w: 1440, h: 900, maximized });
        s.maximized = maximized;
        if !maximized {
            // Only remember the normal (restored) bounds.
            let size = win.inner_size();
            if let Ok(p) = win.outer_position() {
                s = Saved { x: p.x, y: p.y, w: size.width, h: size.height, maximized: false };
            }
        }
        store.set_setting(KEY, &s.encode());
    });
}

/// 64×64 app icon: rounded square with the accent → green gradient used in the side rail.
pub fn icon() -> Icon {
    const N: u32 = 64;
    let (a, b) = ([0x7c_u8, 0x8c, 0xff], [0x4a_u8, 0xde, 0x80]);
    let r = 14.0_f32;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let t = (x + y) as f32 / (2 * (N - 1)) as f32;
            // Distance outside the rounded rectangle, for an antialiased edge.
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (r - fx).max(fx - (N as f32 - r)).max(0.0);
            let dy = (r - fy).max(fy - (N as f32 - r)).max(0.0);
            let outside = (dx * dx + dy * dy).sqrt() - r;
            let alpha = (0.5 - outside).clamp(0.0, 1.0);
            for c in 0..3 {
                rgba.push((a[c] as f32 + (b[c] as f32 - a[c] as f32) * t) as u8);
            }
            rgba.push((alpha * 255.0) as u8);
        }
    }
    Icon::from_rgba(rgba, N, N).expect("valid icon")
}

#[cfg(test)]
mod tests {
    use super::Saved;

    #[test]
    fn roundtrip_and_rejects_tiny() {
        let s = Saved { x: -1200, y: 40, w: 1600, h: 1000, maximized: true };
        assert_eq!(Saved::parse(&s.encode()), Some(s));
        assert_eq!(Saved::parse("0,0,10,10,0"), None);
        assert_eq!(Saved::parse("junk"), None);
    }

    #[test]
    fn icon_builds() {
        let _ = super::icon();
    }
}

/// Fixed interface zoom ("1" = 100%). Window size never changes text size: big windows get
/// more room and spacing (see the wide-window rules in style.css), not bigger text.
/// Older settings saved as "auto" fall back to 100%.
pub static UI_SCALE: GlobalSignal<String> = Signal::global(|| "1".to_string());
pub const SCALE_KEY: &str = "ui.scale";
pub const SCALE_CHOICES: [(&str, &str); 5] = [("0.9", "90%"), ("1", "100%"), ("1.1", "110%"), ("1.25", "125%"), ("1.5", "150%")];

pub fn scale_factor(setting: &str) -> f64 {
    setting.parse::<f64>().map(|f| f.clamp(0.5, 2.0)).unwrap_or(1.0)
}

/// Keeps the webview zoom in sync with the "Interface size" setting.
pub fn use_ui_scale() {
    let store = use_store();
    let win = use_window();
    let applied = use_hook(|| Rc::new(Cell::new(0.0f64)));
    use_hook(|| {
        if let Some(v) = store.setting(SCALE_KEY) {
            *UI_SCALE.write() = if scale_factor(&v) == 1.0 { "1".into() } else { v };
        }
    });
    use_effect(move || {
        let factor = scale_factor(&UI_SCALE.read());
        if (factor - applied.get()).abs() > 0.001 {
            applied.set(factor);
            let _ = win.webview.zoom(factor);
        }
    });
}

#[cfg(test)]
mod scale_tests {
    #[test]
    fn fixed_factors_only() {
        assert_eq!(super::scale_factor("1.25"), 1.25);
        assert_eq!(super::scale_factor("auto"), 1.0);
        assert_eq!(super::scale_factor("9"), 2.0);
    }
}

/// Title bar colours per theme: (caption background, caption text, window border), matching
/// `--bg` / `--text` / `--border` in style.css so the native title bar blends into the app.
fn titlebar_colors(dark: bool) -> ((u8, u8, u8), (u8, u8, u8), (u8, u8, u8)) {
    if dark {
        ((0x0e, 0x10, 0x14), (0xe6, 0xe8, 0xee), (0x26, 0x2b, 0x36))
    } else {
        ((0xf4, 0xf5, 0xf8), (0x17, 0x1a, 0x21), (0xe3, 0xe6, 0xec))
    }
}

/// Colour the real Windows title bar to match the theme (Windows 11; on Windows 10 only the
/// dark/light frame applies). Keeps native snapping, resizing and caption buttons.
pub fn use_themed_titlebar() {
    let ui = crate::state::use_ui();
    let win = use_window();
    use_effect(move || {
        let dark = *ui.theme.read() == crate::state::Theme::Dark;
        #[cfg(windows)]
        {
            use dioxus::desktop::tao::platform::windows::WindowExtWindows;
            use windows_sys::Win32::Graphics::Dwm::{
                DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
            };
            let hwnd = win.hwnd() as _;
            let colorref = |(r, g, b): (u8, u8, u8)| -> u32 { r as u32 | (g as u32) << 8 | (b as u32) << 16 };
            let (caption, text, border) = titlebar_colors(dark);
            let set = |attr: i32, value: u32| {
                // SAFETY: valid HWND of this process; value is a 4-byte DWORD/COLORREF/BOOL.
                unsafe { DwmSetWindowAttribute(hwnd, attr as u32, &value as *const u32 as *const _, 4) };
            };
            set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
            set(DWMWA_CAPTION_COLOR, colorref(caption));
            set(DWMWA_TEXT_COLOR, colorref(text));
            set(DWMWA_BORDER_COLOR, colorref(border));
        }
        #[cfg(not(windows))]
        let _ = (&win, dark);
    });
}
