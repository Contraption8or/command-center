use super::{money, PendingFull, Stat, WidgetDef};
use crate::icons;
use crate::state::{use_ui, Action, Command, View};
use crate::store::use_store;
use dioxus::prelude::*;

pub static DEF: WidgetDef = WidgetDef {
    id: "spending",
    title: "Spending",
    icon: icons::WALLET,
    cols: 4,
    rows: 2,
    card,
    full,
    commands,
    search: None,
    open_item: None,
    on_message: None,
};

fn card() -> Element {
    let store = use_store();
    let today = *use_ui().today.read();
    let (s, sym) = store.read(|db| (cc_core::model::summary(db, today), db.setting("currency.symbol").unwrap_or_else(|| "$".into())));
    rsx! {
        div { class: "stat-row",
            Stat { value: money(s.spent_this_month_cents, &sym), label: "this month" }
            Stat { value: money(s.spent_last_month_cents, &sym), label: "last month", class: "muted" }
        }
    }
}

fn full() -> Element {
    rsx! { PendingFull { id: "spending", phase: 6 } }
}

fn commands() -> Vec<Command> {
    vec![Command::new("Go to", icons::WALLET, "Spending", Action::Open(View::Widget("spending"))).keywords("money expenses budget")]
}
