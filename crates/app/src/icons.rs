//! Inline stroke icons (24×24 grid). Add a constant and use `Icon { d: icons::X }`.

use dioxus::prelude::*;

pub const OVERVIEW: &str = "M4 4h7v7H4zM13 4h7v4h-7zM13 10h7v10h-7zM4 13h7v7H4z";
pub const TASKS: &str = "M9 11l3 3 8-8M20 12v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h11";
pub const CALENDAR: &str = "M5 5h14a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM4 10h16M8 3v4M16 3v4";
pub const PROJECTS: &str = "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z";
pub const WALLET: &str = "M4 7h15a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1zM4 7l11-3v3M16 13h1";
pub const HABITS: &str = "M12 3v3M12 18v3M3 12h3M18 12h3M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z";
pub const NOTES: &str = "M6 3h9l4 4v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM14 3v5h5M9 13h6M9 17h4";
pub const SETTINGS: &str = "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z";
pub const SEARCH: &str = "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4";
pub const PLUS: &str = "M12 5v14M5 12h14";
pub const ARROW_LEFT: &str = "M19 12H5M11 6l-6 6 6 6";
pub const EXPAND: &str = "M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7";
pub const SUN: &str = "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4";
pub const DOWNLOAD: &str = "M12 3v12M7 10l5 5 5-5M5 21h14";
pub const UPLOAD: &str = "M12 21V9M7 14l5-5 5 5M5 3h14";
pub const TRASH: &str = "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3";
pub const REFRESH: &str = "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7";
pub const GRIP: &str = "M9 6h.01M9 12h.01M9 18h.01M15 6h.01M15 12h.01M15 18h.01";
pub const CLOSE: &str = "M6 6l12 12M18 6L6 18";
pub const PIN: &str = "M9 4h6l-1 6 3 3v2H7v-2l3-3zM12 15v6";
pub const FLAG: &str = "M5 21V4M5 4h11l-2 4 2 4H5";
pub const ARROW_RIGHT: &str = "M5 12h14M13 6l6 6-6 6";
pub const ARROW_UP: &str = "M12 19V5M6 11l6-6 6 6";
pub const ARROW_DOWN: &str = "M12 5v14M6 13l6 6 6-6";
pub const SPARKLE: &str = "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 3v4M17 5h4M5 17v3M3.5 18.5h3";

#[component]
pub fn Icon(d: &'static str) -> Element {
    rsx! {
        svg { class: "icon", view_box: "0 0 24 24", path { d } }
    }
}
pub const PENCIL: &str = "M4 20h4L19 9l-4-4L4 16v4zM14 6l4 4";
pub const CHECK: &str = "M5 12.5l4.5 4.5L19 7.5";
