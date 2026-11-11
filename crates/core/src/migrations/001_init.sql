-- Command Center schema v1.
-- Conventions: dates are ISO 'YYYY-MM-DD', timestamps ISO-8601 local 'YYYY-MM-DDTHH:MM:SS',
-- money is integer cents, every user table carries `seed` (1 = sample data).

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE projects (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'idea'
                  CHECK (status IN ('idea','planned','active','paused','done')),
    description   TEXT NOT NULL DEFAULT '',
    next_action   TEXT NOT NULL DEFAULT '',
    progress      INTEGER NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    progress_mode TEXT NOT NULL DEFAULT 'manual' CHECK (progress_mode IN ('manual','tasks')),
    color         TEXT NOT NULL DEFAULT '#7c8cff',
    position      INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    seed          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tasks (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    notes         TEXT NOT NULL DEFAULT '',
    project_id    INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    due_date      TEXT,
    scheduled_for TEXT NOT NULL,
    position      INTEGER NOT NULL DEFAULT 0,
    done_at       TEXT,
    carried_count INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    seed          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX tasks_scheduled ON tasks(scheduled_for, position);
CREATE INDEX tasks_project ON tasks(project_id);

CREATE TABLE events (
    id          INTEGER PRIMARY KEY,
    title       TEXT NOT NULL,
    starts_at   TEXT NOT NULL,
    ends_at     TEXT,
    all_day     INTEGER NOT NULL DEFAULT 0,
    location    TEXT NOT NULL DEFAULT '',
    notes       TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'manual',
    external_id TEXT,
    calendar_id TEXT,
    seed        INTEGER NOT NULL DEFAULT 0,
    UNIQUE (source, external_id)
);
CREATE INDEX events_start ON events(starts_at);

CREATE TABLE notes (
    id         INTEGER PRIMARY KEY,
    body_md    TEXT NOT NULL DEFAULT '',
    pinned     INTEGER NOT NULL DEFAULT 0,
    project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    seed       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE expense_categories (
    id                   INTEGER PRIMARY KEY,
    name                 TEXT NOT NULL UNIQUE,
    color                TEXT NOT NULL DEFAULT '#7c8cff',
    monthly_budget_cents INTEGER,
    position             INTEGER NOT NULL DEFAULT 0,
    seed                 INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE expenses (
    id           INTEGER PRIMARY KEY,
    date         TEXT NOT NULL,
    amount_cents INTEGER NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    merchant     TEXT NOT NULL DEFAULT '',
    category_id  INTEGER REFERENCES expense_categories(id) ON DELETE SET NULL,
    source       TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual','csv')),
    import_hash  TEXT UNIQUE,
    seed         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX expenses_date ON expenses(date);

CREATE TABLE import_profiles (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    mapping     TEXT NOT NULL,   -- JSON: column indexes/names, date format, sign convention
    seed        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE category_rules (
    id          INTEGER PRIMARY KEY,
    pattern     TEXT NOT NULL,   -- case-insensitive substring of description/merchant
    category_id INTEGER NOT NULL REFERENCES expense_categories(id) ON DELETE CASCADE,
    seed        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE habits (
    id                INTEGER PRIMARY KEY,
    name              TEXT NOT NULL,
    cadence           TEXT NOT NULL DEFAULT 'daily' CHECK (cadence IN ('daily','weekly')),
    target_per_period INTEGER NOT NULL DEFAULT 1,
    color             TEXT NOT NULL DEFAULT '#4ade80',
    archived          INTEGER NOT NULL DEFAULT 0,
    position          INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    seed              INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE habit_checks (
    habit_id INTEGER NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    date     TEXT NOT NULL,
    seed     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (habit_id, date)
);

CREATE TABLE goals (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    unit          TEXT NOT NULL DEFAULT '',
    start_value   REAL NOT NULL DEFAULT 0,
    current_value REAL NOT NULL DEFAULT 0,
    target_value  REAL NOT NULL,
    deadline      TEXT,
    project_id    INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    created_at    TEXT NOT NULL,
    seed          INTEGER NOT NULL DEFAULT 0
);
