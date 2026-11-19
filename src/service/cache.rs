use crate::repo::snapshot::Snapshot;
use serde::{Serialize, de::DeserializeOwned};

pub(super) enum Source {
    Server,
    Cache(i64),
}

pub(super) fn refresh<T: Serialize + DeserializeOwned, E>(
    snapshot: &Snapshot<'_>,
    fetch: impl FnOnce() -> Result<T, E>,
    unavailable: impl FnOnce(&E) -> bool,
) -> Result<(T, Source), E> {
    match fetch() {
        Ok(value) => {
            match serde_json::to_vec(&value) {
                Ok(bytes) => {
                    if let Err(error) = snapshot.write(Some(&bytes)) {
                        eprintln!("无法保存本地缓存：{error}");
                    }
                }
                Err(error) => eprintln!("无法编码本地缓存：{error}"),
            }
            Ok((value, Source::Server))
        }
        Err(error) if unavailable(&error) => snapshot
            .read(None)
            .ok()
            .flatten()
            .and_then(|(updated, bytes)| {
                serde_json::from_slice(&bytes)
                    .ok()
                    .map(|value| (value, Source::Cache(updated)))
            })
            .ok_or(error),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, process};

    #[test]
    fn cached_pages_are_isolated_and_only_used_for_temporary_failures() {
        let root = env::temp_dir().join(format!("yourdiary-pages-{}", process::id()));
        let path = root.join("yourdiary.sqlite3");
        let page = |owner, key| Snapshot::new(owner, key).at(&path);
        let saved = page("a", "posts/0");
        let offline = || Err::<Vec<i64>, _>("offline");
        let unavailable = |e: &&str| *e == "offline";
        refresh(&saved, || Ok(vec![42_i64]), unavailable).unwrap();
        let (rows, source) = refresh(&saved, offline, unavailable).unwrap();
        assert_eq!(rows, [42]);
        assert!(matches!(source, Source::Cache(_)));
        for other in [page("b", "posts/0"), page("a", "posts/1")] {
            assert!(refresh(&other, offline, unavailable).is_err());
        }
        assert!(refresh(&saved, || Err::<Vec<i64>, _>("unauthorized"), unavailable).is_err());
        assert!(refresh(&saved, || Err::<Vec<i64>, _>("malformed"), unavailable).is_err());
        refresh(&page("a", "posts/0"), || Ok(Vec::<i64>::new()), unavailable).unwrap();
        assert!(refresh(&saved, offline, unavailable).unwrap().0.is_empty());
        page("a", "posts/0").write(None).unwrap();
        saved.write(Some(b"[1]")).unwrap();
        assert!(refresh(&saved, offline, unavailable).is_err());
        let first = page("a", "like/1");
        first.write(Some(b"old")).unwrap();
        let since = first.read(None).unwrap().unwrap().0 + 1;
        page("b", "like/1").write(Some(b"foreign")).unwrap();
        page("a", "like/2").write(Some(b"new")).unwrap();
        page("a", "like/3").write(None).unwrap();
        page("a", "like/4").write(Some(b"other page")).unwrap();
        let targets = ["like/1", "like/2", "like/3"].map(String::from);
        let found = saved.read_related(&targets, since).unwrap();
        assert_eq!(
            found,
            std::collections::HashMap::from([("like/2".into(), b"new".to_vec())])
        );
        fs::remove_dir_all(root).unwrap();
    }
}
