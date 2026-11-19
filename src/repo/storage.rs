use std::{
    error::Error,
    fs::{DirBuilder, OpenOptions, Permissions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use super::DATABASE_PATH;

pub(super) fn now_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}

pub(super) fn open(path: &Path) -> Result<Connection, Box<dyn Error>> {
    let legacy = (path == Path::new(DATABASE_PATH)).then(|| {
        gtk::glib::user_data_dir()
            .join(crate::APP_ID)
            .join("session.sqlite3")
    });
    open_with_legacy(path, legacy.as_deref())
}

fn open_with_legacy(
    path: &Path,
    platform_session: Option<&Path>,
) -> Result<Connection, Box<dyn Error>> {
    let mut db = open_private(path)?;
    db.pragma_update(None, "secure_delete", true)?;
    let version =
        |db: &Connection| db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0));
    if version(&db)? == 2 {
        return Ok(db);
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let attached = match version(&tx)? {
        2 => {
            tx.commit()?;
            return Ok(db);
        }
        1 => false,
        0 => initialize_v1(&tx, path, platform_session)?,
        _ => return Err("数据库版本高于当前客户端支持的版本".into()),
    };
    tx.execute_batch(super::preferences::sql("initialize"))?;
    tx.pragma_update(Some("main"), "user_version", 2)?;
    tx.commit()?;
    if attached {
        db.execute_batch("DETACH DATABASE legacy")?;
    }
    Ok(db)
}

fn initialize_v1(
    tx: &Connection,
    path: &Path,
    platform_session: Option<&Path>,
) -> Result<bool, Box<dyn Error>> {
    tx.execute_batch(sql_block(
        include_str!("../../resources/auth_session.sql"),
        "initialize",
    ))?;
    tx.execute_batch(sql_block(
        include_str!("../../resources/diary_cache.sql"),
        "initialize",
    ))?;
    tx.execute_batch(
        "CREATE TABLE snapshot (
        owner BLOB NOT NULL, resource TEXT NOT NULL, body BLOB, updated INTEGER NOT NULL,
        PRIMARY KEY (owner, resource)
    ) WITHOUT ROWID;",
    )?;

    let directory = path.parent().unwrap_or(Path::new("."));
    let local = directory.join("session.sqlite3");
    let mut import_platform = true;
    if local.try_exists()? {
        let legacy =
            Connection::open_with_flags(&local, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let token = read_legacy_session(&legacy)?;
        import_platform = token.is_none()
            && legacy.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? == 0;
        if let Some(token) = token {
            tx.execute("INSERT INTO session (id,token) VALUES (1, ?1)", [token])?;
        }
    }
    if import_platform
        && let Some(old) = platform_session
        && old.try_exists()?
    {
        let legacy = Connection::open_with_flags(old, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if let Some(token) = read_legacy_session(&legacy)? {
            tx.execute("INSERT INTO session (id,token) VALUES (1, ?1)", [token])?;
        }
    }
    let diaries = directory.join("diaries.sqlite3");
    let attached = diaries.try_exists()?;
    if attached {
        let mut url = reqwest::Url::from_file_path(diaries.canonicalize()?)
            .map_err(|_| "旧日记数据库路径无效")?;
        url.set_query(Some("mode=ro"));
        tx.execute("ATTACH DATABASE ?1 AS legacy", [url.as_str()])?;
        tx.execute_batch(
            "INSERT INTO main.diary_content (account_id,author_id,diary_id,revision,content,sha256)
            SELECT account_id,author_id,diary_id,revision,content,sha256 FROM legacy.diary_content;
            INSERT INTO main.diary_cache_initialized (account_id)
            SELECT account_id FROM legacy.diary_cache_initialized;",
        )?;
    }
    Ok(attached)
}

fn read_legacy_session(db: &Connection) -> rusqlite::Result<Option<String>> {
    db.query_row("SELECT token FROM session WHERE id = 1", [], |r| r.get(0))
        .optional()
}

fn open_private(path: &Path) -> Result<Connection, Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?
        .set_permissions(Permissions::from_mode(0o600))?;
    Ok(Connection::open(path)?)
}

pub(crate) fn sql_block<'a>(source: &'a str, name: &str) -> &'a str {
    source
        .split("\n-- name: ")
        .find_map(|block| {
            block
                .strip_prefix("-- name: ")
                .unwrap_or(block)
                .split_once('\n')
                .filter(|(key, _)| *key == name)
                .map(|(_, statement)| statement)
        })
        .expect("缺少 SQL 语句")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, process};

    #[test]
    fn v1_upgrade_keeps_data_and_reopens_with_saved_preferences() {
        let root = env::temp_dir().join(format!("yourdiary-prefs-{}", process::id()));
        let path = root.join("yourdiary.sqlite3");
        let old = open_private(&path).unwrap();
        old.execute_batch(
            "CREATE TABLE session (id INTEGER PRIMARY KEY, token TEXT);
            INSERT INTO session VALUES (1,'sample'); PRAGMA user_version=1",
        )
        .unwrap();
        drop(old);
        let db = open(&path).unwrap();
        assert_eq!(read_legacy_session(&db).unwrap().as_deref(), Some("sample"));
        for value in ["dark", "light"] {
            db.execute(
                super::super::preferences::sql("save"),
                ("appearance", value),
            )
            .unwrap();
        }
        drop(db);
        let db = open(&path).unwrap();
        let saved: String = db
            .query_row(
                super::super::preferences::sql("read"),
                ["appearance"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(saved, "light");
        drop(db);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrates_legacy_databases_atomically_and_only_once() {
        let root = env::temp_dir().join(format!("yourdiary-migrate-{}", process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("yourdiary.sqlite3");
        let legacy = root.join("session.sqlite3");
        let platform = root.join("platform.sqlite3");
        let old_diaries = root.join("diaries.sqlite3");
        for (path, token) in [(&legacy, "local"), (&platform, "platform")] {
            let db = Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE session (id INTEGER PRIMARY KEY, token TEXT NOT NULL)")
                .unwrap();
            db.execute("INSERT INTO session VALUES (1, ?1)", [token])
                .unwrap();
        }
        let old = Connection::open(&old_diaries).unwrap();
        // 一个损坏或不完整的旧库不能留下半次迁移。
        assert!(open_with_legacy(&path, Some(&platform)).is_err());
        let probe = Connection::open(&path).unwrap();
        assert_eq!(
            probe
                .query_row("SELECT count(*) FROM sqlite_schema", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(probe);
        old.execute_batch(
            "CREATE TABLE diary_content (
            account_id INTEGER, author_id INTEGER, diary_id INTEGER, revision INTEGER,
            content TEXT, sha256 BLOB, PRIMARY KEY(account_id,author_id,diary_id));
            CREATE TABLE diary_cache_initialized(account_id INTEGER PRIMARY KEY);
            INSERT INTO diary_content VALUES (1,2,3,4,'正文',x'1234');
            INSERT INTO diary_cache_initialized VALUES (1)",
        )
        .unwrap();
        drop(old);
        let db = open_with_legacy(&path, Some(&platform)).unwrap();
        assert_eq!(read_legacy_session(&db).unwrap().as_deref(), Some("local"));
        assert!(db.query_row("SELECT EXISTS (SELECT 1 FROM diary_content JOIN diary_cache_initialized USING(account_id)
            WHERE account_id=1 AND author_id=2 AND diary_id=3 AND revision=4 AND content='正文' AND sha256=x'1234')", [], |r| r.get::<_, bool>(0)).unwrap());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        db.execute("DELETE FROM session", []).unwrap();
        drop(db);
        assert!(
            read_legacy_session(&open_with_legacy(&path, Some(&platform)).unwrap())
                .unwrap()
                .is_none()
        );

        // 已注销的本地旧会话优先于更早的系统目录凭证。
        let local = Connection::open(&legacy).unwrap();
        local
            .execute_batch("DELETE FROM session; PRAGMA user_version=1")
            .unwrap();
        let empty = open_with_legacy(&root.join("empty.sqlite3"), Some(&platform)).unwrap();
        assert!(read_legacy_session(&empty).unwrap().is_none());
        drop(empty);
        drop(local);
        fs::remove_file(legacy).unwrap();
        let recovered =
            open_with_legacy(&root.join("platform-only.sqlite3"), Some(&platform)).unwrap();
        assert_eq!(
            read_legacy_session(&recovered).unwrap().as_deref(),
            Some("platform")
        );
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }
}
