-- ========================
-- Core infrastructure
-- ========================

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

-- ========================
-- Economy
-- ========================

CREATE TABLE IF NOT EXISTS stats (
    stat_name TEXT PRIMARY KEY,
    stat_value REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS stock (
    resource_name TEXT PRIMARY KEY,
    amount INTEGER NOT NULL
);

-- ========================
-- Map layout
-- ========================

CREATE TABLE IF NOT EXISTS grid_coords (
    entity_id INTEGER,
    x INTEGER,
    y INTEGER,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS grid_imprints (
    id INTEGER PRIMARY KEY,
    shape TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS world_positions (
    entity_id INTEGER PRIMARY KEY,
    x REAL NOT NULL,
    y REAL NOT NULL,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

-- ========================
-- Buildings (shared)
-- ========================

CREATE TABLE IF NOT EXISTS integrity_points (
    entity_id INTEGER PRIMARY KEY,
    current REAL NOT NULL,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS disabled_by_player (
    entity_id INTEGER PRIMARY KEY,
    FOREIGN KEY(entity_id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS walls (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS main_bases (
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

CREATE TABLE IF NOT EXISTS exploration_centers (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- The nullable forging_* columns hold an in-progress craft so a forge resumes mid-job
-- across save/load; both are NULL when the forge is idle.
CREATE TABLE IF NOT EXISTS forges (
    id INTEGER PRIMARY KEY,
    forging_shard_type TEXT,
    forging_remaining_secs REAL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS quantum_fields (
    id INTEGER PRIMARY KEY,
    current_layer INTEGER NOT NULL,
    current_layer_progress REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- ========================
-- Towers
-- ========================

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

CREATE TABLE IF NOT EXISTS tower_fields (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- ========================
-- Shards
-- ========================

CREATE TABLE IF NOT EXISTS entity_shards (
    shard_target_id INTEGER NOT NULL,
    shard_index INTEGER NOT NULL,
    shard_type TEXT NOT NULL,
    PRIMARY KEY (shard_target_id, shard_index)
);

CREATE TABLE IF NOT EXISTS shard_inventory (
    shard_type TEXT PRIMARY KEY,
    count INTEGER NOT NULL
);

-- Shard types the player has unlocked for forging (membership only).
CREATE TABLE IF NOT EXISTS shard_blueprints (
    shard_type TEXT PRIMARY KEY
);

-- ========================
-- Projectiles
-- ========================

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

CREATE TABLE IF NOT EXISTS ripples (
    id INTEGER PRIMARY KEY,
    max_radius REAL NOT NULL,
    current_radius REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- ========================
-- Wisps
-- ========================

CREATE TABLE IF NOT EXISTS wisps (
    id INTEGER PRIMARY KEY,
    wisp_type TEXT NOT NULL,
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

-- ========================
-- Objectives
-- ========================

-- Objective roots. `activated_by` is the trigger entity that activates this
-- objective (nullable: terminal objectives don't need one).
CREATE TABLE IF NOT EXISTS objectives (
    id INTEGER PRIMARY KEY,
    id_name TEXT NOT NULL,
    state TEXT NOT NULL,
    activated_by INTEGER,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- Kill-wisps goals. `current` is the kill count since activation (always present).
CREATE TABLE IF NOT EXISTS goal_kill_wisps (
    id INTEGER PRIMARY KEY,
    objective_id INTEGER NOT NULL,
    state TEXT NOT NULL,
    target INTEGER NOT NULL,
    current INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(id) REFERENCES entities(id),
    FOREIGN KEY(objective_id) REFERENCES entities(id)
);

-- Clear-quantum-fields goals. Counter is derived at runtime from the live world
-- (count of QuantumField entities with QuantumFieldSolved) — not persisted.
CREATE TABLE IF NOT EXISTS goal_clear_quantum_fields (
    id INTEGER PRIMARY KEY,
    objective_id INTEGER NOT NULL,
    state TEXT NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id),
    FOREIGN KEY(objective_id) REFERENCES entities(id)
);

-- Time allowance restrictions (maintenance polarity: satisfied at activation,
-- failed on expiry). `elapsed` is always present (0.0 until activated).
CREATE TABLE IF NOT EXISTS restriction_time_allowance (
    id INTEGER PRIMARY KEY,
    objective_id INTEGER NOT NULL,
    state TEXT NOT NULL,
    seconds REAL NOT NULL,
    elapsed REAL NOT NULL DEFAULT 0.0,
    FOREIGN KEY(id) REFERENCES entities(id),
    FOREIGN KEY(objective_id) REFERENCES entities(id)
);

-- StartGame trigger — singleton. `fired` prevents re-firing on mid-game reload.
CREATE TABLE IF NOT EXISTS trigger_start_game (
    id INTEGER PRIMARY KEY,
    fired INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(id) REFERENCES entities(id)
);

CREATE TABLE IF NOT EXISTS summonings (
    id INTEGER PRIMARY KEY,
    summoning_json TEXT NOT NULL,
    produced INTEGER NOT NULL,
    next_spawn_time REAL NOT NULL,
    is_active INTEGER NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- ========================
-- Research
-- ========================

-- One row per research instance present on the map.
CREATE TABLE IF NOT EXISTS researches (
    id INTEGER PRIMARY KEY,
    research_type TEXT NOT NULL,
    duration_secs REAL NOT NULL,
    -- NULL when not in flight (not started or completed); a fraction in [0,1] while in progress.
    progress REAL,
    is_active INTEGER NOT NULL DEFAULT 0,
    is_completed INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- A research's (editable) cost, one row per resource. `essence_type` is set only for essence costs.
CREATE TABLE IF NOT EXISTS research_costs (
    research_id INTEGER NOT NULL,
    resource_kind TEXT NOT NULL,
    essence_type TEXT,
    amount INTEGER NOT NULL,
    FOREIGN KEY(research_id) REFERENCES researches(id)
);

-- Outcome kind: grant a shard blueprint. One table per outcome kind; `research_id` links to its
-- research via the entity map.
CREATE TABLE IF NOT EXISTS research_outcome_shard_blueprints (
    id INTEGER PRIMARY KEY,
    research_id INTEGER NOT NULL,
    shard_type TEXT NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);
