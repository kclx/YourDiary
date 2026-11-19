-- name: initialize
CREATE TABLE preferences (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

-- name: read
SELECT value FROM preferences WHERE key = ?1;

-- name: save
INSERT INTO preferences (key, value) VALUES (?1, ?2)
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
