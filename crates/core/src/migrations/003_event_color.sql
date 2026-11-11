-- Per-event colour (hex like #7c8cff); NULL = the theme's accent.
ALTER TABLE events ADD COLUMN color TEXT;
