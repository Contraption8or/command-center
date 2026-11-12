//! Settings view plus the data actions it shares with the command palette.

use super::keys::{HotkeyState, DEFAULT_HOTKEY};
use crate::icons::{self, Icon};
use crate::state::{run_action, use_ui, Action, Theme, Ui};
use crate::store::{use_store, Store};
use cc_core::{backup, seed};
use dioxus::prelude::*;
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

#[component]
pub fn Settings() -> Element {
    let store = use_store();
    let ui = use_ui();
    let hk = use_context::<HotkeyState>();
    let mut hk_draft = use_signal(|| hk.accel.peek().clone());
    let mut currency = use_signal(|| store.setting("currency.symbol").unwrap_or_else(|| "$".into()));

    let (path, schema, seed_rows, seed_state) =
        store.read(|db| (db.path().display().to_string(), db.schema_version().unwrap_or(0), seed::seed_row_count(db), seed::state(db)));
    let theme = *ui.theme.read();
    let h12 = *ui.h12.read();
    let s_clock = store.clone();

    let s1 = store.clone();
    let s2 = store.clone();
    let s3 = store.clone();
    let s4 = store.clone();
    let s5 = store.clone();
    let s6 = store.clone();
    let s_dir = store.clone();
    let s_std = store.clone();
    let folder = store.read(|db| db.path().parent().map(|p| p.display().to_string()).unwrap_or_default());
    let custom_dir = cc_core::db::data_dir_is_custom() && std::env::var(cc_core::db::DATA_DIR_ENV).is_err();
    let s7 = store.clone();
    let s8 = store.clone();
    let s9 = store.clone();
    let last_auto = store.read(|db| db.setting("backup.last_auto")).unwrap_or_else(|| "none yet".into());

    rsx! {
        div { class: "full",
            div { class: "panel",
                h3 { "Appearance" }
                div { class: "row",
                    span { class: "label", "Theme" }
                    div { class: "value",
                        div { class: "seg",
                            button { class: if theme == Theme::Dark { "on" }, onclick: move |_| if theme != Theme::Dark { run_action(Action::ToggleTheme, &s1, ui) }, "Dark" }
                            button { class: if theme == Theme::Light { "on" }, onclick: move |_| if theme != Theme::Light { run_action(Action::ToggleTheme, &s2, ui) }, "Light" }
                        }
                    }
                }
                div { class: "row",
                    span { class: "label", "Clock" }
                    div { class: "value",
                        div { class: "seg",
                            for (label, twelve) in [("12-hour", true), ("24-hour", false)] {
                                button {
                                    class: if h12 == twelve { "on" },
                                    onclick: { let s = s_clock.clone(); move |_| {
                                        let mut sig = ui.h12;
                                        sig.set(twelve);
                                        s.set_setting("clock", if twelve { "12" } else { "24" });
                                    } },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                div { class: "row",
                    span { class: "label", "Interface size" }
                    div { class: "value",
                        div { class: "seg",
                            for (value, label) in crate::window_state::SCALE_CHOICES {
                                button {
                                    class: if crate::window_state::UI_SCALE() == value { "on" },
                                    onclick: { let s = s_clock.clone(); move |_| {
                                        *crate::window_state::UI_SCALE.write() = value.to_string();
                                        s.set_setting(crate::window_state::SCALE_KEY, value);
                                    } },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                div { class: "row",
                    span { class: "label", "Currency symbol" }
                    div { class: "value",
                        input { class: "text-input", style: "min-width: 60px; width: 80px;", value: "{currency}",
                            oninput: move |e| { currency.set(e.value()); s3.set_setting("currency.symbol", &e.value()); } }
                    }
                }
            }
            super::layout_editor::LayoutEditor {}
            crate::widgets::assistant::AssistantSettings {}
            div { class: "panel",
                h3 { "Shortcuts" }
                div { class: "row",
                    span { class: "label", "Global quick add" }
                    div { class: "value",
                        input { class: "text-input", value: "{hk_draft}", oninput: move |e| hk_draft.set(e.value()) }
                        button { class: "btn", onclick: move |_| {
                            let v = hk_draft.peek().trim().to_string();
                            s4.set_setting("hotkey.quick_add", &v);
                            let mut a = hk.accel; a.set(v);
                        }, "Apply" }
                        button { class: "btn", onclick: move |_| {
                            s5.write(|db| db.delete_setting("hotkey.quick_add")).ok();
                            hk_draft.set(DEFAULT_HOTKEY.into());
                            let mut a = hk.accel; a.set(DEFAULT_HOTKEY.into());
                        }, "Reset" }
                        if let Some(err) = hk.error.read().clone() { span { class: "chip warn", "{err}" } }
                    }
                }
            }
            div { class: "panel",
                h3 { "Data" }
                div { class: "row",
                    span { class: "label", "Data folder" }
                    div { class: "value",
                        span { class: "path", title: "{path}", "{folder}" }
                        button { class: "btn", onclick: move |_| change_data_folder(&s_dir, ui), "Change…" }
                        if custom_dir {
                            button { class: "btn", title: "Go back to %APPDATA%\\CommandCenter\\data", onclick: move |_| use_standard_folder(&s_std, ui), "Use standard" }
                        }
                        button { class: "btn", onclick: move |_| open_data_folder(&s6, ui), "Open" }
                    }
                }
                div { class: "row",
                    span { class: "label", "Backup" }
                    div { class: "value",
                        button { class: "btn", onclick: move |_| export_backup(&s7, ui), Icon { d: icons::DOWNLOAD } " Export…" }
                        button { class: "btn", onclick: move |_| restore_backup(&s8, ui), Icon { d: icons::UPLOAD } " Restore…" }
                    }
                }
                div { class: "row",
                    span { class: "label", "Automatic backups" }
                    div { class: "value",
                        span { class: "muted", "Daily, last {cc_core::backup::AUTO_KEEP} kept · latest {last_auto}" }
                        button { class: "btn", onclick: move |_| open_backup_folder(&s9, ui), "Open folder" }
                    }
                }
                div { class: "row",
                    span { class: "label", "Sample data" }
                    div { class: "value",
                        span { class: "muted",
                            match seed_state {
                                seed::SeedState::Seeded if seed_rows > 0 => format!("{seed_rows} sample rows"),
                                _ => "None".to_string(),
                            }
                        }
                        SampleButtons {}
                    }
                }
                crate::updates::UpdateSettings {}
                div { class: "row",
                    span { class: "label", "Schema" }
                    div { class: "value muted", "v{schema}" }
                }
            }
        }
    }
}

#[component]
fn SampleButtons() -> Element {
    let store = use_store();
    let ui = use_ui();
    let a = store.clone();
    let b = store.clone();
    rsx! {
        button { class: "btn danger", onclick: move |_| clear_sample(&a, ui), "Clear" }
        button { class: "btn", onclick: move |_| reload_sample(&b, ui), "Reload" }
    }
}

fn confirm(title: &str, text: &str) -> bool {
    MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title(title)
        .set_description(text)
        .set_buttons(MessageButtons::OkCancel)
        .show()
        == MessageDialogResult::Ok
}

pub fn export_backup(store: &Store, ui: Ui) {
    let name = backup::suggested_name(*ui.today.peek());
    let Some(dest) = FileDialog::new()
        .set_title("Export backup")
        .set_file_name(&name)
        .add_filter("Command Center backup", &["ccbackup"])
        .save_file()
    else {
        return;
    };
    match store.peek(|db| db.export_to(&dest)) {
        Ok(()) => ui.notify(format!("Backup saved · {}", dest.file_name().unwrap_or_default().to_string_lossy())),
        Err(e) => ui.error(format!("Export failed: {e:#}")),
    }
}

pub fn restore_backup(store: &Store, mut ui: Ui) {
    let Some(src) = FileDialog::new()
        .set_title("Restore from backup")
        .add_filter("Command Center backup", &["ccbackup", "db", "sqlite"])
        .pick_file()
    else {
        return;
    };
    if !confirm("Restore backup", "Replace all current data with this backup? A safety copy of your current data is saved first.") {
        return;
    }
    // Safety copy next to the live database.
    let safety = store.peek(|db| {
        let dir = db.path().parent().map(|p| p.join("backups")).unwrap_or_default();
        std::fs::create_dir_all(&dir).ok();
        let file = dir.join(format!("pre-restore-{}.ccbackup", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        db.export_to(&file).map(|_| file)
    });
    if let Err(e) = safety {
        ui.error(format!("Could not save a safety copy, restore cancelled: {e:#}"));
        return;
    }
    match store.write(|db| db.restore_from(&src)) {
        Ok(()) => {
            ui.theme.set(Theme::parse(store.setting("theme").as_deref()));
            ui.notify("Backup restored");
        }
        Err(e) => ui.error(format!("Restore failed: {e:#}")),
    }
}

pub fn open_data_folder(store: &Store, ui: Ui) {
    let dir = store.peek(|db| db.path().parent().map(|p| p.to_path_buf()));
    if let Some(dir) = dir {
        if let Err(e) = std::process::Command::new("explorer").arg(&dir).spawn() {
            ui.error(format!("Could not open folder: {e}"));
        }
    }
}

pub fn clear_sample(store: &Store, ui: Ui) {
    if !confirm("Clear sample data", "Delete all sample rows? Anything you added yourself is kept.") {
        return;
    }
    match store.write(seed::clear) {
        Ok(()) => ui.notify("Sample data cleared"),
        Err(e) => ui.error(format!("Clear failed: {e:#}")),
    }
}

pub fn reload_sample(store: &Store, ui: Ui) {
    if !confirm("Reload sample data", "Replace the sample rows with a fresh copy? Edits made to sample rows are lost; your own data is kept.") {
        return;
    }
    let today = *ui.today.peek();
    match store.write(|db| seed::reload(db, today)) {
        Ok(()) => ui.notify("Sample data reloaded"),
        Err(e) => ui.error(format!("Reload failed: {e:#}")),
    }
}

pub fn open_backup_folder(store: &Store, ui: Ui) {
    let dir = store.peek(cc_core::backup::backup_dir);
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = std::process::Command::new("explorer").arg(&dir).spawn() {
        ui.error(format!("Could not open folder: {e}"));
    }
}

/// Pick a new data folder (Settings → Data). Your data is copied there, or the folder's
/// existing data is used; then the app restarts on it.
pub fn change_data_folder(store: &Store, ui: Ui) {
    let Some(dir) = FileDialog::new().set_title("Choose where Command Center keeps its data").pick_folder() else { return };
    switch_data_folder(store, ui, dir);
}

pub fn use_standard_folder(store: &Store, ui: Ui) {
    match cc_core::db::standard_data_dir() {
        Ok(dir) => switch_data_folder(store, ui, dir),
        Err(e) => ui.error(format!("{e:#}")),
    }
}

fn switch_data_folder(store: &Store, ui: Ui, dir: std::path::PathBuf) {
    let current = store.peek(|db| db.path().parent().map(|p| p.to_path_buf())).unwrap_or_default();
    if dir == current {
        ui.notify("Already using that folder");
        return;
    }
    if std::env::var(cc_core::db::DATA_DIR_ENV).is_ok() {
        ui.error("The data folder is set by COMMAND_CENTER_DATA for this run");
        return;
    }
    let has_data = dir.join(cc_core::db::DB_FILE).exists();
    let text = if has_data {
        format!(
            "{} already has Command Center data.\n\nSwitch to it? Command Center will restart. Your current data stays in {}.",
            dir.display(),
            current.display()
        )
    } else {
        format!(
            "Copy your data to {} and use that folder from now on?\n\nCommand Center will restart. The current copy stays in {} as a fallback.",
            dir.display(),
            current.display()
        )
    };
    if !confirm("Change data folder", &text) {
        return;
    }
    let result = store.peek(|db| db.move_to(&dir)).and_then(|_| {
        let standard = cc_core::db::standard_data_dir()?;
        cc_core::db::remember_data_dir(if dir == standard { None } else { Some(&dir) })
    });
    match result {
        Ok(()) => {
            let _ = store.peek(|db| db.checkpoint());
            crate::restart();
        }
        Err(e) => ui.error(format!("Couldn't change the data folder: {e:#}")),
    }
}
