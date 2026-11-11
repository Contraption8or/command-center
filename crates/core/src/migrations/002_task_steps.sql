-- Step tasks: "Edit 3 hours" = 3 steps; each step checked off adds to its progress.
ALTER TABLE tasks ADD COLUMN steps_total INTEGER NOT NULL DEFAULT 1 CHECK (steps_total BETWEEN 1 AND 99);
ALTER TABLE tasks ADD COLUMN steps_done  INTEGER NOT NULL DEFAULT 0 CHECK (steps_done >= 0);
