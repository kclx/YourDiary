use super::{
    DATABASE_PATH,
    storage::{now_micros, open},
};
use aws_lc_rs::digest::{Digest, SHA256, digest};
use rusqlite::OptionalExtension;
use std::{error::Error, path::Path};

/// 每个归属和资源只保留最近的结果。请求开始时间防止慢请求覆盖较新的刷新或失效操作。
pub(crate) struct Snapshot<'a> {
    path: &'a Path,
    owner: Digest,
    resource: &'a str,
    started: i64,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn new(owner: &str, resource: &'a str) -> Self {
        Self {
            path: Path::new(DATABASE_PATH),
            owner: digest(&SHA256, owner.as_bytes()),
            resource,
            started: now_micros(),
        }
    }

    pub(crate) fn read(
        &self,
        max_age_secs: Option<i64>,
    ) -> Result<Option<(i64, Vec<u8>)>, Box<dyn Error>> {
        let since = max_age_secs.map(|age| self.started - age * 1_000_000);
        Ok(open(self.path)?
            .query_row(
                "SELECT updated, body FROM snapshot WHERE owner=?1 AND resource=?2 AND body IS NOT NULL
             AND (?3 IS NULL OR updated >= ?3)",
                (self.owner.as_ref(), self.resource, since),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub(crate) fn read_related(
        &self,
        resources: &[String],
        since: i64,
    ) -> Result<std::collections::HashMap<String, Vec<u8>>, Box<dyn Error>> {
        if resources.is_empty() {
            return Ok(Default::default());
        }
        let db = open(self.path)?;
        let mut query = db.prepare(
            "SELECT resource, body FROM snapshot WHERE owner=?1 AND updated>=?2 AND body IS NOT NULL
             AND resource IN (SELECT value FROM json_each(?3))"
        )?;
        Ok(query
            .query_map(
                (
                    self.owner.as_ref(),
                    since,
                    serde_json::to_string(resources)?,
                ),
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn find_treehole_post(
        &self,
        id: i64,
    ) -> Result<Option<(String, String)>, Box<dyn Error>> {
        Ok(open(self.path)?.query_row(
            "SELECT s.resource, p.value FROM snapshot s, json_each(CAST(s.body AS TEXT)) p
             WHERE s.owner=?1 AND s.resource GLOB 'treehole/posts/*' AND json_extract(p.value, '$.id')=?2
             ORDER BY s.updated DESC LIMIT 1",
            (self.owner.as_ref(), id), |row| Ok((row.get(0)?, row.get(1)?))
        ).optional()?)
    }

    pub(crate) fn write(&self, body: Option<&[u8]>) -> Result<(), Box<dyn Error>> {
        open(self.path)?.execute(
            "INSERT INTO snapshot(owner,resource,body,updated) VALUES (?1,?2,?3,?4)
             ON CONFLICT(owner,resource) DO UPDATE SET body=excluded.body, updated=excluded.updated
             WHERE snapshot.updated <= excluded.updated",
            (self.owner.as_ref(), self.resource, body, self.started),
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn at(mut self, path: &'a Path) -> Self {
        self.path = path;
        self
    }
}
