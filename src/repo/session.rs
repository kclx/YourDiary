use std::{error::Error, path::PathBuf};

use rusqlite::OptionalExtension;

use super::{
    DATABASE_PATH,
    storage::{open, sql_block},
};

pub(crate) struct StoredSession {
    pub token: String,
    pub account_id: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn load(&self) -> Result<Option<StoredSession>, Box<dyn Error>> {
        Ok(open(&self.path)?
            .query_row(sql("load"), [], |r| {
                Ok(StoredSession {
                    token: r.get(0)?,
                    account_id: r.get(1)?,
                })
            })
            .optional()?)
    }

    pub(crate) fn save(
        &self,
        session: &str,
        account_id: Option<i64>,
    ) -> Result<(), Box<dyn Error>> {
        open(&self.path)?.execute(sql("save"), (session, account_id))?;
        Ok(())
    }

    pub(crate) fn forget(&self, session: &str) -> Result<(), Box<dyn Error>> {
        open(&self.path)?.execute(sql("forget"), [session])?;
        Ok(())
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self {
            path: DATABASE_PATH.into(),
        }
    }
}

fn sql(name: &str) -> &'static str {
    sql_block(include_str!("../../resources/auth_session.sql"), name)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, process};

    #[test]
    fn forget_only_removes_the_matching_session() {
        let path = env::temp_dir()
            .join(format!("yourdiary-session-{}", process::id()))
            .join("yourdiary.sqlite3");
        let store = SessionStore { path: path.clone() };
        assert!(store.load().unwrap().is_none());
        store.save("first", Some(1)).unwrap();
        store.save("second", Some(2)).unwrap();
        store.forget("first").unwrap();
        let saved = store.load().unwrap().unwrap();
        assert_eq!(
            (saved.token.as_str(), saved.account_id),
            ("second", Some(2))
        );
        store.forget("second").unwrap();
        assert!(store.load().unwrap().is_none());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
