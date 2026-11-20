pub(crate) mod diary;
pub(crate) mod login;
pub(crate) mod profile;
pub(crate) mod treehole;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub(super) fn bool_flag<'de, D: Deserializer<'de>>(field: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(field)? {
        Value::Bool(value) => value,
        Value::Number(value) => value
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .is_some_and(|n| n != 0),
        Value::String(value) => {
            value.eq_ignore_ascii_case("true") || value.parse::<i32>().is_ok_and(|n| n != 0)
        }
        _ => false,
    })
}

pub(super) fn optional_object<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).ok())
}
