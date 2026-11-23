ALTER TABLE lobs ADD COLUMN progress REAL NOT NULL DEFAULT 0;
ALTER TABLE lobs DROP COLUMN launch_at;

CREATE TABLE IF NOT EXISTS projectile_launch_counters (
    entity_id INTEGER PRIMARY KEY,
    launch_at REAL NOT NULL,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);
