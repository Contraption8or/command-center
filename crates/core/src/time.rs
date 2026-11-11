use chrono::{Local, NaiveDate};

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Local timestamp in the storage format `YYYY-MM-DDTHH:MM:SS`.
pub fn now_ts() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
