-- An event can be checked off: that creates a completed task, linked here. Deleting the
-- task (or un-checking it) makes the event not done again.
ALTER TABLE events ADD COLUMN done_task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL;
