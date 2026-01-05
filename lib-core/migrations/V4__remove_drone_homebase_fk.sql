-- Remove home_base_id foreign key constraint from expedition_drones2
-- Keep only the id -> entities FK, drop the home_base_id -> entities FK
-- This allows saving drones without requiring home_base to be saved first

CREATE TABLE expedition_drones2_new (
    id INTEGER PRIMARY KEY,
    home_base_id INTEGER NOT NULL,
    state INTEGER NOT NULL,
    mission_target_id INTEGER,
    heading REAL NOT NULL,
    waypoint_x REAL NOT NULL,
    waypoint_y REAL NOT NULL,
    fuel_current REAL NOT NULL,
    fuel_max REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

INSERT INTO expedition_drones2_new SELECT * FROM expedition_drones2;

DROP TABLE expedition_drones2;
DROP TABLE expedition_drones;

ALTER TABLE expedition_drones2_new RENAME TO expedition_drones;
