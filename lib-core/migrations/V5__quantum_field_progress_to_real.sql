-- Change current_layer_progress from INTEGER to REAL for float-based scan progress

-- SQLite doesn't support ALTER COLUMN, so we recreate the table
CREATE TABLE quantum_fields_new (
    id INTEGER PRIMARY KEY,
    current_layer INTEGER NOT NULL,
    current_layer_progress REAL NOT NULL,
    FOREIGN KEY(id) REFERENCES entities(id)
);

-- Migrate data, casting INTEGER to REAL
INSERT INTO quantum_fields_new (id, current_layer, current_layer_progress)
SELECT id, current_layer, CAST(current_layer_progress AS REAL) FROM quantum_fields;

DROP TABLE quantum_fields;

ALTER TABLE quantum_fields_new RENAME TO quantum_fields;
