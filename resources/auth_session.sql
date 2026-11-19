-- name: initialize
PRAGMA secure_delete = ON;

CREATE TABLE IF NOT EXISTS session (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    token TEXT NOT NULL CHECK (length(token) > 0),
    account_id INTEGER
);

-- name: load
SELECT token, account_id FROM session WHERE id = 1;

-- name: save
INSERT INTO session (id, token, account_id) VALUES (1, ?1, ?2)
ON CONFLICT (id) DO UPDATE SET token = excluded.token, account_id = excluded.account_id;

-- name: forget
DELETE FROM session WHERE id = 1 AND token = ?1;
