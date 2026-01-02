-- Remove is_expedition_target column from quantum_fields
-- This column is no longer needed as drones now track their own mission_target
-- and valid targets are identified by ExpeditionZone component

-- SQLite doesn't support DROP COLUMN directly, so we recreate the table
CREATE TABLE quantum_fields_new (
    id INTEGER PRIMARY KEY,
    current_layer INTEGER NOT NULL,
    current_layer_progress INTEGER NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

INSERT INTO quantum_fields_new (id, current_layer, current_layer_progress)
SELECT id, current_layer, current_layer_progress FROM quantum_fields;

DROP TABLE quantum_fields;

ALTER TABLE quantum_fields_new RENAME TO quantum_fields;
