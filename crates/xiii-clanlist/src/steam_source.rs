use crate::SteamCacheRecord;
use std::fs;
use std::path::PathBuf;

pub trait SteamRosterSource {
    fn name(&self) -> &'static str;
    fn read_records(&self) -> Result<Vec<SteamCacheRecord>, String>;
    fn writes_legacy_state(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct LegacyJsonSteamRosterSource {
    pub cache_path: PathBuf,
}

impl SteamRosterSource for LegacyJsonSteamRosterSource {
    fn name(&self) -> &'static str {
        "legacy steam_roster_cache.json"
    }

    fn read_records(&self) -> Result<Vec<SteamCacheRecord>, String> {
        let text = fs::read_to_string(&self.cache_path)
            .map_err(|err| format!("failed to read {}: {err}", self.cache_path.display()))?;
        crate::parse_steam_cache(&text)
    }
}

#[derive(Debug, Clone)]
pub struct GoogleSheetsSteamRosterSource {
    pub service_account_file_redacted: &'static str,
    pub sheet_id_redacted: &'static str,
}

impl SteamRosterSource for GoogleSheetsSteamRosterSource {
    fn name(&self) -> &'static str {
        "Google Sheets read-only Steam roster"
    }

    fn read_records(&self) -> Result<Vec<SteamCacheRecord>, String> {
        Err("Google Sheets read-only Steam source is not implemented yet; use legacy JSON cache fallback".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{GoogleSheetsSteamRosterSource, SteamRosterSource};

    #[test]
    fn google_source_is_explicitly_deferred() {
        let source = GoogleSheetsSteamRosterSource {
            service_account_file_redacted: "<SET>",
            sheet_id_redacted: "<SET>",
        };

        assert!(source
            .read_records()
            .unwrap_err()
            .contains("not implemented"));
        assert!(!source.writes_legacy_state());
    }
}
