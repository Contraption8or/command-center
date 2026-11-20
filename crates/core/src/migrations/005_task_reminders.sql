-- Task deadlines can have a time, a daily reminder, and a reward used in reminder text.
ALTER TABLE tasks ADD COLUMN due_time TEXT;
ALTER TABLE tasks ADD COLUMN remind_daily TEXT;
ALTER TABLE tasks ADD COLUMN reward TEXT NOT NULL DEFAULT '';
-- Reminders already shown, so each fires once (keys like "ev:12:2026-11-20T15:00:00").
CREATE TABLE reminders_sent (
    key     TEXT PRIMARY KEY,
    sent_at TEXT NOT NULL
);
