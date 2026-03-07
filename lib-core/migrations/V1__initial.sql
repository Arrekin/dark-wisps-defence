CREATE TABLE IF NOT EXISTS entities (
    id INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS map_info (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS game_clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    elapsed REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE IF NOT EXISTS walls (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS grid_coords (
    entity_id INTEGER,
    x INTEGER,
    y INTEGER,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS main_bases (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS exploration_centers (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS energy_relays (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS mining_complexes (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS dark_ores (
    id INTEGER PRIMARY KEY,
    amount INTEGER NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS grid_imprints (
    id INTEGER PRIMARY KEY,
    shape TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS quantum_fields (
    id INTEGER PRIMARY KEY,
    current_layer INTEGER NOT NULL,
    current_layer_progress REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS healths (
    entity_id INTEGER PRIMARY KEY,
    current REAL NOT NULL,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS world_positions (
    entity_id INTEGER PRIMARY KEY,
    x REAL NOT NULL,
    y REAL NOT NULL,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS disabled_by_player (
    entity_id INTEGER PRIMARY KEY,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS expedition_drones (
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

CREATE TABLE IF NOT EXISTS cannonballs (
    id INTEGER PRIMARY KEY,
    target_x REAL NOT NULL,
    target_y REAL NOT NULL,
    damage REAL NOT NULL,
    initial_distance REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS laser_darts (
    id INTEGER PRIMARY KEY,
    target_wisp_id INTEGER,
    vector_x REAL NOT NULL,
    vector_y REAL NOT NULL,
    damage REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS rockets (
    id INTEGER PRIMARY KEY,
    target_wisp_id INTEGER,
    rotation_z REAL NOT NULL,
    damage REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS tower_cannons (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS tower_blasters (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS tower_rocket_launchers (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS tower_emitters (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS ripples (
    id INTEGER PRIMARY KEY,
    max_radius REAL NOT NULL,
    current_radius REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS brittle_effects (
    id INTEGER PRIMARY KEY,
    target_id INTEGER NOT NULL,
    source_id INTEGER,
    expires_at REAL,
    damage_multiplier REAL NOT NULL DEFAULT 1.0,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS wisps (
    id INTEGER PRIMARY KEY,
    wisp_type TEXT NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS objectives (
    id INTEGER PRIMARY KEY,
    id_name TEXT NOT NULL,
    objective_type TEXT NOT NULL,
    activation_event TEXT NOT NULL,
    state TEXT NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS objective_kill_wisps (
    id INTEGER PRIMARY KEY,
    target_amount INTEGER NOT NULL,
    started_amount INTEGER NOT NULL,
    FOREIGN KEY(id) REFERENCES objectives(id)
);

CREATE TABLE IF NOT EXISTS stats (
    stat_name TEXT PRIMARY KEY,
    stat_value REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS stock (
    resource_name TEXT PRIMARY KEY,
    amount INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS summonings (
    id INTEGER PRIMARY KEY,
    summoning_json TEXT NOT NULL,
    produced INTEGER NOT NULL,
    next_spawn_time REAL NOT NULL,
    is_active INTEGER NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS upgrade_levels (
    entity_id INTEGER NOT NULL,
    upgrade_type TEXT NOT NULL,
    current_level INTEGER NOT NULL,
    PRIMARY KEY (entity_id, upgrade_type),
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);
