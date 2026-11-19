use super::{
    DATABASE_PATH,
    storage::{open, sql_block},
};
use rusqlite::OptionalExtension;
use std::{error::Error, path::Path};

pub(super) fn sql(name: &str) -> &'static str {
    sql_block(include_str!("../../resources/preferences.sql"), name)
}

pub(crate) fn read(key: &str) -> Result<Option<String>, Box<dyn Error>> {
    Ok(open(Path::new(DATABASE_PATH))?
        .query_row(sql("read"), [key], |row| row.get(0))
        .optional()?)
}

pub(crate) fn save(key: &str, value: &str) -> Result<(), Box<dyn Error>> {
    open(Path::new(DATABASE_PATH))?.execute(sql("save"), (key, value))?;
    Ok(())
}
