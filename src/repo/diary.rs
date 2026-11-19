use super::{
    DATABASE_PATH,
    storage::{now_micros, open, sql_block},
};
use aws_lc_rs::digest::{SHA256, digest};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{collections::HashMap, error::Error, path::Path};

pub(crate) struct FullDiary {
    pub content: String,
    pub revision: Option<i64>,
}

pub(crate) struct CachedExcerpt {
    pub revision: Option<i64>,
    pub text: String,
    pub cut_before: bool,
    pub cut_after: bool,
}

pub(crate) fn search_cached(
    account: i64,
    query: &str,
) -> Result<HashMap<(i64, i64), CachedExcerpt>, Box<dyn Error>> {
    let Some(db) = open_cached_read_only()? else {
        return Ok(HashMap::new());
    };
    Ok(DiaryCache(db).search(account, query)?)
}

pub(crate) fn cached_body(
    account: i64,
    author: i64,
    id: i64,
    revision: Option<i64>,
) -> Option<String> {
    let db = open_cached_read_only().ok().flatten()?;
    let full = DiaryCache(db).load(account, author, id).ok().flatten()?;
    revision
        .is_none_or(|expected| full.revision >= Some(expected))
        .then_some(full.content)
}

fn open_cached_read_only() -> Result<Option<Connection>, Box<dyn Error>> {
    let path = Path::new(DATABASE_PATH);
    if !path.try_exists()? {
        return Ok(None);
    }
    Ok(Some(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?))
}

pub(crate) struct DiaryCache(Connection);

impl DiaryCache {
    pub(crate) fn open() -> Result<Self, Box<dyn Error>> {
        Ok(Self(open(Path::new(DATABASE_PATH))?))
    }

    #[cfg(test)]
    pub(crate) fn at(path: &Path) -> Result<Self, Box<dyn Error>> {
        Ok(Self(open(path)?))
    }

    pub(crate) fn confirm_read(
        &self,
        account: i64,
        author: i64,
        id: i64,
        revision: i64,
    ) -> rusqlite::Result<()> {
        self.0.execute(
            sql("confirm_read"),
            (account, author, id, revision, now_micros()),
        )?;
        Ok(())
    }

    pub(crate) fn confirmed_since(
        &self,
        account: i64,
        since: i64,
    ) -> rusqlite::Result<HashMap<(i64, i64), i64>> {
        self.0
            .prepare(sql("confirmed_since"))?
            .query_map((account, since), |r| {
                Ok(((r.get(0)?, r.get(1)?), r.get(2)?))
            })?
            .collect()
    }

    pub(crate) fn load(
        &self,
        account: i64,
        author: i64,
        id: i64,
    ) -> rusqlite::Result<Option<FullDiary>> {
        self.0
            .query_row(sql("load"), (account, author, id), |row| {
                Ok(FullDiary {
                    content: row.get(0)?,
                    revision: row.get(1)?,
                })
            })
            .optional()
    }

    pub(crate) fn save(
        &self,
        account: i64,
        author: i64,
        id: i64,
        full: &FullDiary,
    ) -> rusqlite::Result<()> {
        save(
            &self.0,
            account,
            author,
            id,
            full.content.as_str(),
            full.revision,
        )
    }

    pub(crate) fn search(
        &self,
        account: i64,
        query: &str,
    ) -> rusqlite::Result<HashMap<(i64, i64), CachedExcerpt>> {
        // ponytail: 逐篇扫描当前账号的正文，库明显变大或实测变慢后再考虑 FTS5。
        let mut statement = self.0.prepare(sql("search"))?;
        statement
            .query_map((account, query), |row| {
                Ok((
                    (row.get(0)?, row.get(1)?),
                    CachedExcerpt {
                        revision: row.get(2)?,
                        text: row.get(3)?,
                        cut_before: row.get(4)?,
                        cut_after: row.get(5)?,
                    },
                ))
            })?
            .collect()
    }

    pub(crate) fn initialized(&self, account: i64) -> rusqlite::Result<bool> {
        self.0
            .query_row(sql("initialized"), [account], |row| row.get(0))
    }

    pub(crate) fn complete(&self, account: i64) -> rusqlite::Result<()> {
        self.0.execute(sql("complete"), [account])?;
        Ok(())
    }

    pub(crate) fn revision(
        &self,
        account: i64,
        author: i64,
        id: i64,
    ) -> rusqlite::Result<Option<i64>> {
        self.0
            .prepare_cached(sql("revision"))?
            .query_row((account, author, id), |row| row.get(0))
            .optional()
            .map(Option::flatten)
    }

    pub(crate) fn save_batch<'a>(
        &self,
        account: i64,
        entries: impl IntoIterator<Item = (i64, i64, &'a str, Option<i64>)>,
    ) -> rusqlite::Result<()> {
        let transaction = self.0.unchecked_transaction()?;
        for (author, id, content, revision) in entries {
            save(&transaction, account, author, id, content, revision)?;
        }
        transaction.commit()
    }
}

fn save(
    db: &Connection,
    account: i64,
    author: i64,
    id: i64,
    content: &str,
    revision: Option<i64>,
) -> rusqlite::Result<()> {
    let hash = digest(&SHA256, content.as_bytes());
    db.execute(
        sql("save"),
        params![account, author, id, revision, content, hash.as_ref()],
    )?;
    Ok(())
}

fn sql(name: &str) -> &'static str {
    sql_block(include_str!("../../resources/diary_cache.sql"), name)
}
