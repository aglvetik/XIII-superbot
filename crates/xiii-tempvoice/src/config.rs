#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempVoiceRuntimeConfig {
    pub delete_after_seconds: u64,
    pub legacy_db_path: String,
}
