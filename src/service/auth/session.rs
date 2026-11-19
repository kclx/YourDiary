use crate::service::profile::Profile;
use std::sync::atomic::{AtomicBool, Ordering};

/// 当前会话。恢复已保存的 token 时，个人资料需要重新获取。
pub(crate) struct Session {
    token: String,
    logged_out: AtomicBool,
    pub(super) account_id: Option<i64>,
    profile: Option<Profile>,
}

impl Session {
    pub(super) fn new(token: String, account_id: Option<i64>, profile: Option<Profile>) -> Self {
        Session {
            token,
            logged_out: AtomicBool::new(false),
            account_id,
            profile,
        }
    }

    pub(crate) fn restore(
        store: &super::SessionStore,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        Ok(store
            .load()?
            .map(|stored| Self::new(stored.token, stored.account_id, None)))
    }

    pub(crate) fn snapshot<'a>(&self, key: &'a str) -> crate::repo::snapshot::Snapshot<'a> {
        let owner = match self.account_id {
            Some(id) => format!("account/{id}"),
            None => format!("session/{}", self.token),
        };
        crate::repo::snapshot::Snapshot::new(&owner, key)
    }

    pub(crate) fn end(&self) {
        self.logged_out.store(true, Ordering::Release);
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.logged_out.load(Ordering::Acquire)
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    pub(crate) fn account_id(&self) -> Option<i64> {
        self.account_id
    }

    pub(crate) fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }
}
