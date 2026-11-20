-- name: initialize
CREATE TABLE IF NOT EXISTS diary_content (
    account_id INTEGER NOT NULL,
    author_id INTEGER NOT NULL,
    diary_id INTEGER NOT NULL,
    revision INTEGER,
    content TEXT NOT NULL,
    sha256 BLOB NOT NULL,
    read_revision INTEGER,
    read_at INTEGER,
    PRIMARY KEY (account_id, author_id, diary_id)
);

CREATE TABLE IF NOT EXISTS diary_cache_initialized (
    account_id INTEGER PRIMARY KEY
);

-- name: initialized
SELECT EXISTS(SELECT 1 FROM diary_cache_initialized WHERE account_id = ?1);

-- name: complete
INSERT OR IGNORE INTO diary_cache_initialized (account_id) VALUES (?1);

-- name: revisions
SELECT author_id, diary_id, revision FROM diary_content WHERE account_id = ?1 AND revision IS NOT NULL;

-- name: load
SELECT content, revision FROM diary_content WHERE account_id = ?1 AND author_id = ?2 AND diary_id = ?3;

-- name: search
WITH matches AS (
    SELECT author_id, diary_id, revision, content,
           instr(lower(content), lower(?2)) AS hit
    FROM diary_content WHERE account_id = ?1
)
SELECT author_id, diary_id, revision,
       substr(content, max(1, hit - 18), hit + length(?2) + 28 - max(1, hit - 18)),
       hit > 19,
       substr(content, hit + length(?2) + 28, 1) <> ''
FROM matches WHERE hit > 0;

-- name: save
INSERT INTO diary_content (account_id, author_id, diary_id, revision, content, sha256)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT (account_id, author_id, diary_id) DO UPDATE SET
    revision = excluded.revision,
    content = CASE WHEN diary_content.sha256 = excluded.sha256 THEN diary_content.content ELSE excluded.content END,
    sha256 = excluded.sha256
WHERE (diary_content.revision IS NULL OR excluded.revision >= diary_content.revision)
  AND (diary_content.revision IS NOT excluded.revision OR diary_content.sha256 <> excluded.sha256);

-- name: confirm_read
UPDATE diary_content SET read_revision = ?4, read_at = ?5
WHERE account_id = ?1 AND author_id = ?2 AND diary_id = ?3
  AND (read_revision IS NULL OR read_revision <= ?4);

-- name: confirmed_since
SELECT author_id, diary_id, read_revision FROM diary_content
WHERE account_id = ?1 AND read_at >= ?2;
