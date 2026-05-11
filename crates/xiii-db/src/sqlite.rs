pub use crate::{SqlitePool, SQLITE_READ_ONLY_URI_HINT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySqliteAccessPlan {
    pub path: String,
    pub read_only: bool,
    pub query_only: bool,
}

impl LegacySqliteAccessPlan {
    pub fn read_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            read_only: true,
            query_only: true,
        }
    }
}
