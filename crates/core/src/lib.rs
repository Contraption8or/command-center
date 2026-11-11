//! Command Center core: storage, domain logic, import/export. No UI code lives here,
//! so everything in this crate is unit-testable with `cargo test -p cc-core`.

pub mod ai;
pub mod backup;
pub mod calendar;
pub mod db;
pub mod markdown;
pub mod model;
pub mod quick_add;
pub mod seed;
pub mod time;
pub mod tz;
pub mod update;

pub use db::Db;
