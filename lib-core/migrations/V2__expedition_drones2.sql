CREATE TABLE expedition_drones2 (
    id INTEGER PRIMARY KEY,
    home_base_id INTEGER NOT NULL,
    state INTEGER NOT NULL,
    mission_target_id INTEGER,
    heading REAL NOT NULL,
    waypoint_x REAL NOT NULL,
    waypoint_y REAL NOT NULL,
    fuel_current REAL NOT NULL,
    fuel_max REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id),
    FOREIGN KEY(home_base_id) REFERENCES entities(id)
);
