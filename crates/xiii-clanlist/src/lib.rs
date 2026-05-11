use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use xiii_config::ConfigLoad;
use xiii_core::{
    EnvDependency, ModuleId, ModuleManifest, Report, SchedulerJobDescriptor, StateDependency,
};

pub mod steam_source;

pub fn manifest() -> ModuleManifest {
    ModuleManifest::new(
        ModuleId::Clanlist,
        "XIII Clanlist",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\XIII\\XIII-clanlist",
        "medium",
    )
    .with_state(StateDependency::json_directory("opt/XIII/XIII-clanlist/data", "roster message IDs and Steam cache JSON files"))
    .with_state(StateDependency::json_file("opt/XIII/XIII-clanlist/data/main_roster_message_ids.json", "main roster panel message 1498766315299799185"))
    .with_state(StateDependency::json_file("opt/XIII/XIII-clanlist/data/admin_roster_message_ids.json", "admin roster panel message 1498766321867821218"))
    .with_state(StateDependency::json_file("opt/XIII/XIII-clanlist/data/steam_roster_message_ids.json", "Steam roster panel message 1500086435506683954"))
    .with_state(StateDependency::json_file("opt/XIII/XIII-clanlist/data/steam_roster_cache.json", "19 cached Steam roster records"))
    .with_env(EnvDependency::new(Some("SERVER_ID"), "XIII_GUILD_ID", true, false, "target guild"))
    .with_env(EnvDependency::new(Some("MAIN_LIST_CHANNEL_ID"), "CLANLIST_MAIN_CHANNEL_ID", true, false, "main roster channel"))
    .with_env(EnvDependency::new(Some("ADMIN_LIST_CHANNEL_ID"), "CLANLIST_ADMIN_CHANNEL_ID", true, false, "admin roster channel"))
    .with_env(EnvDependency::new(Some("STEAM_LIST_CHANNEL_ID"), "CLANLIST_STEAM_CHANNEL_ID", true, false, "Steam roster channel"))
    .with_env(EnvDependency::new(Some("STEAM_ACTIVE_ROLE_ID"), "CLANLIST_STEAM_ACTIVE_ROLE_ID", true, false, "Steam active role"))
    .with_env(EnvDependency::new(Some("GOOGLE_SERVICE_ACCOUNT_FILE"), "CLANLIST_GOOGLE_SERVICE_ACCOUNT_FILE", true, true, "clanlist Google service account file"))
    .with_env(EnvDependency::new(Some("GOOGLE_SHEET_ID"), "CLANLIST_GOOGLE_SHEET_ID", true, true, "clanlist Steam Google Sheet ID"))
    .with_job(SchedulerJobDescriptor::startup("clanlist_bootstrap_targets", "app/bot.py:112").mutating())
    .with_job(SchedulerJobDescriptor::interval("clanlist_auto_refresh", 600, "app/services/update_scheduler.py:119").mutating())
    .with_note("No Discord commands/components; preserve JSON message IDs before enabling writer.")
    .with_note("Dry-run capability: clanlist-preview reads legacy JSON/cache only and never calls Discord or Google.")
    .with_note("Read-only Discord diagnostic: discord-readonly-clanlist-snapshot fetches roles/members through Discord HTTP only when --allow-discord-read is present.")
    .with_note("Read-only Discord diagnostic: clanlist-target-message-check fetches only /users/@me and the three exact panel messages; Discord writes disabled.")
    .with_note("Production-ready Clanlist-only runtime: run-clanlist edits only the three fresh Superbot-owned panel messages from data/clanlist_panel_state.json after explicit read/write/confirm gates.")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamPreviewMode {
    Auto,
    Include,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClanlistPreviewOptions {
    pub steam: SteamPreviewMode,
}

impl Default for ClanlistPreviewOptions {
    fn default() -> Self {
        Self {
            steam: SteamPreviewMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSafety {
    pub read_only: bool,
    pub discord_login: bool,
    pub discord_gateway: bool,
    pub discord_http: bool,
    pub google_sheets: bool,
    pub legacy_json_writes: bool,
    pub migrations: bool,
}

impl PreviewSafety {
    pub fn offline_read_only() -> Self {
        Self {
            read_only: true,
            discord_login: false,
            discord_gateway: false,
            discord_http: false,
            google_sheets: false,
            legacy_json_writes: false,
            migrations: false,
        }
    }

    pub fn discord_http_read_only() -> Self {
        Self {
            read_only: true,
            discord_login: false,
            discord_gateway: false,
            discord_http: true,
            google_sheets: false,
            legacy_json_writes: false,
            migrations: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanlistRosterTarget {
    pub channel_id: u64,
    pub message_id: u64,
    pub role_order: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanlistSteamTarget {
    pub channel_id: u64,
    pub message_id: u64,
    pub active_role_id: u64,
    pub cached_records: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamCacheRecord {
    pub discord_id: String,
    pub steam_id64: String,
    pub last_display_name: Option<String>,
    pub last_status: Option<String>,
    pub first_seen_at: Option<String>,
    pub last_seen_in_sheet_at: Option<String>,
    pub last_seen_active_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanlistPreviewTargets {
    pub main: ClanlistRosterTarget,
    pub admin: ClanlistRosterTarget,
    pub steam: ClanlistSteamTarget,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistPreviewModel {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub targets: ClanlistPreviewTargets,
    pub steam_cache_count: usize,
    pub steam_records: Vec<SteamCacheRecord>,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistPreviewOutput {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub targets: Option<ClanlistPreviewTargets>,
    pub steam_cache_count: Option<usize>,
    pub steam_records: Vec<SteamCacheRecord>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistPreviewResult {
    pub report: Report,
    pub model: Option<ClanlistPreviewModel>,
}

impl ClanlistPreviewResult {
    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordRoleSnapshotInput {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordMemberSnapshotInput {
    pub user_id: u64,
    pub display_name: String,
    pub role_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotMemberSummary {
    pub user_id: u64,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotMemberRoleMatch {
    pub user_id: u64,
    pub display_name: String,
    pub matching_role_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotRosterRole {
    pub role_id: u64,
    pub role_name: Option<String>,
    pub member_count: Option<usize>,
    pub members: Vec<SnapshotMemberSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistDiscordSnapshotModel {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub guild_id: u64,
    pub targets: ClanlistPreviewTargets,
    pub role_count: usize,
    pub member_count: Option<usize>,
    pub configured_main_role_ids: Vec<u64>,
    pub configured_admin_role_ids: Vec<u64>,
    pub missing_configured_roles: Vec<u64>,
    pub main_roles: Vec<SnapshotRosterRole>,
    pub admin_roles: Vec<SnapshotRosterRole>,
    pub members_with_multiple_configured_main_roles: Vec<SnapshotMemberRoleMatch>,
    pub members_with_multiple_configured_admin_roles: Vec<SnapshotMemberRoleMatch>,
    pub members_with_multiple_configured_roster_roles: Vec<SnapshotMemberRoleMatch>,
    pub members_with_none_of_configured_roster_roles: Vec<SnapshotMemberSummary>,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistDiscordSnapshotOutput {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub guild_id: Option<u64>,
    pub targets: Option<ClanlistPreviewTargets>,
    pub role_count: Option<usize>,
    pub member_count: Option<usize>,
    pub configured_main_role_ids: Vec<u64>,
    pub configured_admin_role_ids: Vec<u64>,
    pub missing_configured_roles: Vec<u64>,
    pub main_roles: Vec<SnapshotRosterRole>,
    pub admin_roles: Vec<SnapshotRosterRole>,
    pub members_with_multiple_configured_main_roles: Vec<SnapshotMemberRoleMatch>,
    pub members_with_multiple_configured_admin_roles: Vec<SnapshotMemberRoleMatch>,
    pub members_with_multiple_configured_roster_roles: Vec<SnapshotMemberRoleMatch>,
    pub members_with_none_of_configured_roster_roles: Vec<SnapshotMemberSummary>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistDiscordSnapshotResult {
    pub report: Report,
    pub model: Option<ClanlistDiscordSnapshotModel>,
    pub safety: PreviewSafety,
}

impl ClanlistDiscordSnapshotResult {
    pub fn no_discord(report: Report) -> Self {
        Self {
            report,
            model: None,
            safety: PreviewSafety::offline_read_only(),
        }
    }

    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderTarget {
    pub channel_id: u64,
    pub message_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedHeaderPreview {
    pub title: &'static str,
    pub description: String,
    pub footer_template: &'static str,
    pub marker_url: &'static str,
    pub color_hex: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedEmbedChunk {
    pub title: String,
    pub description: String,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedMemberLine {
    pub position: usize,
    pub user_id: u64,
    pub display_name: String,
    pub rendered_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedRoleSection {
    pub role_id: u64,
    pub role_name: String,
    pub member_count: usize,
    pub members: Vec<RenderedMemberLine>,
    pub embed_chunks: Vec<RenderedEmbedChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedRosterPanel {
    pub panel_name: &'static str,
    pub target: RenderTarget,
    pub header: RenderedHeaderPreview,
    pub total_members: Option<usize>,
    pub role_sections: Vec<RenderedRoleSection>,
    pub omitted_role_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedSteamEntry {
    pub position: usize,
    pub discord_id: String,
    pub user_id: Option<u64>,
    pub display_name: Option<String>,
    pub steam_id64: String,
    pub status: String,
    pub rendered_entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedSteamBlock {
    pub title: String,
    pub entries: Vec<RenderedSteamEntry>,
    pub embed_chunks: Vec<RenderedEmbedChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedSteamPanel {
    pub panel_name: &'static str,
    pub target: RenderTarget,
    pub source: &'static str,
    pub header: RenderedHeaderPreview,
    pub active_count: Option<usize>,
    pub excluded_count: Option<usize>,
    pub unknown_member_count: usize,
    pub active_block: RenderedSteamBlock,
    pub excluded_block: RenderedSteamBlock,
    pub unknown_member_entries: Vec<RenderedSteamEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistRenderPreviewModel {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub guild_id: u64,
    pub role_count: usize,
    pub member_count: Option<usize>,
    pub main_panel: RenderedRosterPanel,
    pub admin_panel: RenderedRosterPanel,
    pub steam_panel: Option<RenderedSteamPanel>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistRenderPreviewOutput {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub guild_id: Option<u64>,
    pub role_count: Option<usize>,
    pub member_count: Option<usize>,
    pub main_panel: Option<RenderedRosterPanel>,
    pub admin_panel: Option<RenderedRosterPanel>,
    pub steam_panel: Option<RenderedSteamPanel>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistRenderPreviewResult {
    pub report: Report,
    pub model: Option<ClanlistRenderPreviewModel>,
    pub safety: PreviewSafety,
}

impl ClanlistRenderPreviewResult {
    pub fn no_discord(report: Report) -> Self {
        Self {
            report,
            model: None,
            safety: PreviewSafety::offline_read_only(),
        }
    }

    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderTextOptions {
    pub max_members_per_section: usize,
}

impl Default for RenderTextOptions {
    fn default() -> Self {
        Self {
            max_members_per_section: TEXT_MEMBER_PREVIEW_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WritePlanCheck {
    pub status: String,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WritePlanRenderSummary {
    pub guild_id: u64,
    pub role_count: usize,
    pub member_count: Option<usize>,
    pub main_total_members: Option<usize>,
    pub admin_total_members: Option<usize>,
    pub steam_active_records: Option<usize>,
    pub steam_excluded_records: Option<usize>,
    pub steam_unknown_member_records: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedWriteOperation {
    pub operation_type: &'static str,
    pub allowed: bool,
    pub reason: &'static str,
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub message_id: u64,
    pub expected_title: String,
    pub expected_embed_count: usize,
    pub expected_message_chunk_count: usize,
    pub expected_total_label: String,
    pub expected_total: Option<usize>,
    pub safety_checks: Vec<String>,
    pub rollback_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistWritePlanModel {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub render_summary: WritePlanRenderSummary,
    pub planned_operations: Vec<PlannedWriteOperation>,
    pub checks: Vec<WritePlanCheck>,
    pub rollback_notes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistWritePlanOutput {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub render_summary: Option<WritePlanRenderSummary>,
    pub planned_operations: Vec<PlannedWriteOperation>,
    pub checks: Vec<WritePlanCheck>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub rollback_notes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistWritePlanResult {
    pub report: Report,
    pub model: Option<ClanlistWritePlanModel>,
    pub safety: PreviewSafety,
}

impl ClanlistWritePlanResult {
    pub fn no_discord(report: Report) -> Self {
        Self {
            report,
            model: None,
            safety: PreviewSafety::offline_read_only(),
        }
    }

    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetMessageTarget {
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub message_id: u64,
    pub expected_title: String,
    pub expected_marker_url: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetMessageObservationInput {
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub message_id: u64,
    pub exists: bool,
    pub failure_reason: Option<String>,
    pub author_id: Option<u64>,
    pub embed_count: Option<usize>,
    pub first_embed_title: Option<String>,
    pub first_embed_footer_text: Option<String>,
    pub first_embed_footer_icon_url: Option<String>,
    pub first_embed_marker_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetMessageCheck {
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub message_id: u64,
    pub message_exists: bool,
    pub current_bot_user_id: u64,
    pub message_author_id: Option<u64>,
    pub is_current_bot_author: Option<bool>,
    pub editable_by_current_bot: bool,
    pub embed_count: Option<usize>,
    pub expected_title: String,
    pub actual_first_embed_title: Option<String>,
    pub title_roughly_matches: Option<bool>,
    pub first_embed_footer_text: Option<String>,
    pub first_embed_footer_icon_url: Option<String>,
    pub first_embed_marker_url: Option<String>,
    pub expected_marker_url: &'static str,
    pub status: String,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistTargetMessageCheckModel {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub current_bot_user_id: u64,
    pub target_checks: Vec<TargetMessageCheck>,
    pub all_targets_editable_candidates: bool,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistTargetMessageCheckOutput {
    pub mode: &'static str,
    pub safety: PreviewSafety,
    pub current_bot_user_id: Option<u64>,
    pub target_checks: Vec<TargetMessageCheck>,
    pub all_targets_editable_candidates: Option<bool>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistTargetMessageCheckResult {
    pub report: Report,
    pub model: Option<ClanlistTargetMessageCheckModel>,
    pub safety: PreviewSafety,
}

impl ClanlistTargetMessageCheckResult {
    pub fn no_discord(report: Report) -> Self {
        Self {
            report,
            model: None,
            safety: PreviewSafety::offline_read_only(),
        }
    }

    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapSafety {
    pub dry_run: bool,
    pub discord_gateway: bool,
    pub discord_http: bool,
    pub discord_message_creates: bool,
    pub discord_message_edits: bool,
    pub discord_message_deletes: bool,
    pub discord_role_modifications: bool,
    pub discord_dms: bool,
    pub google_sheets: bool,
    pub legacy_json_writes: bool,
    pub legacy_db_writes: bool,
    pub migrations: bool,
    pub allowed_mentions_disabled: bool,
}

impl BootstrapSafety {
    pub fn new(dry_run: bool) -> Self {
        Self {
            dry_run,
            discord_gateway: false,
            discord_http: true,
            discord_message_creates: !dry_run,
            discord_message_edits: false,
            discord_message_deletes: false,
            discord_role_modifications: false,
            discord_dms: false,
            google_sheets: false,
            legacy_json_writes: false,
            legacy_db_writes: false,
            migrations: false,
            allowed_mentions_disabled: true,
        }
    }

    pub fn no_discord() -> Self {
        Self {
            dry_run: true,
            discord_gateway: false,
            discord_http: false,
            discord_message_creates: false,
            discord_message_edits: false,
            discord_message_deletes: false,
            discord_role_modifications: false,
            discord_dms: false,
            google_sheets: false,
            legacy_json_writes: false,
            legacy_db_writes: false,
            migrations: false,
            allowed_mentions_disabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BootstrapEmbedPayload {
    pub title: String,
    pub description: String,
    pub footer_text: String,
    pub marker_url: &'static str,
    pub color_hex: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BootstrapMessagePayload {
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub legacy_message_id: u64,
    pub content: Option<String>,
    pub allowed_mentions_disabled: bool,
    pub embeds: Vec<BootstrapEmbedPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BootstrapOperation {
    pub operation_type: &'static str,
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub legacy_message_id: u64,
    pub new_message_id: Option<u64>,
    pub allowed: bool,
    pub status: String,
    pub expected_title: String,
    pub expected_embed_count: usize,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BootstrapOperationOutcome {
    pub panel_name: &'static str,
    pub new_message_id: Option<u64>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapRenderSummary {
    pub guild_id: u64,
    pub role_count: usize,
    pub member_count: Option<usize>,
    pub main_total_members: Option<usize>,
    pub admin_total_members: Option<usize>,
    pub steam_active_records: Option<usize>,
    pub steam_excluded_records: Option<usize>,
    pub steam_unknown_member_records: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapLegacyMessageIds {
    pub main: u64,
    pub admin: u64,
    pub steam: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClanlistPanelStateTarget {
    pub channel_id: u64,
    pub message_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClanlistPanelStateRenderSummary {
    pub main_total_members: Option<usize>,
    pub admin_total_members: Option<usize>,
    pub steam_active_records: Option<usize>,
    pub steam_excluded_records: Option<usize>,
    pub steam_unknown_member_records: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClanlistPanelStateMessageIds {
    pub main: u64,
    pub admin: u64,
    pub steam: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClanlistPanelState {
    pub created_at_utc: String,
    pub guild_id: u64,
    pub bot_user_id: u64,
    pub source: String,
    pub main: ClanlistPanelStateTarget,
    pub admin: ClanlistPanelStateTarget,
    pub steam: ClanlistPanelStateTarget,
    pub render_summary: ClanlistPanelStateRenderSummary,
    pub old_legacy_message_ids: BootstrapLegacyMessageIds,
    pub warning: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_render_summary: Option<ClanlistPanelStateRenderSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_update_message_ids: Option<ClanlistPanelStateMessageIds>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistBootstrapNewPanelsModel {
    pub mode: &'static str,
    pub safety: BootstrapSafety,
    pub dry_run: bool,
    pub guild_id: u64,
    pub bot_user_id: u64,
    pub render_summary: BootstrapRenderSummary,
    pub old_legacy_message_ids: BootstrapLegacyMessageIds,
    pub payloads: Vec<BootstrapMessagePayload>,
    pub operations: Vec<BootstrapOperation>,
    pub state_file_path: Option<String>,
    pub partial_recovery_file_path: Option<String>,
    pub manual_next_steps: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistBootstrapNewPanelsOutput {
    pub mode: &'static str,
    pub safety: BootstrapSafety,
    pub dry_run: bool,
    pub guild_id: Option<u64>,
    pub bot_user_id: Option<u64>,
    pub render_summary: Option<BootstrapRenderSummary>,
    pub old_legacy_message_ids: Option<BootstrapLegacyMessageIds>,
    pub payloads: Vec<BootstrapMessagePayload>,
    pub operations: Vec<BootstrapOperation>,
    pub state_file_path: Option<String>,
    pub partial_recovery_file_path: Option<String>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub manual_next_steps: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistBootstrapNewPanelsResult {
    pub report: Report,
    pub model: Option<ClanlistBootstrapNewPanelsModel>,
    pub safety: BootstrapSafety,
}

impl ClanlistBootstrapNewPanelsResult {
    pub fn no_discord(report: Report) -> Self {
        Self {
            report,
            model: None,
            safety: BootstrapSafety::no_discord(),
        }
    }

    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSafety {
    pub dry_run: bool,
    pub discord_gateway: bool,
    pub discord_http: bool,
    pub discord_message_creates: bool,
    pub discord_message_edits: bool,
    pub discord_message_deletes: bool,
    pub discord_role_modifications: bool,
    pub discord_dms: bool,
    pub google_sheets: bool,
    pub legacy_json_writes: bool,
    pub legacy_db_writes: bool,
    pub migrations: bool,
    pub allowed_mentions_disabled: bool,
}

impl UpdateSafety {
    pub fn new(dry_run: bool) -> Self {
        Self {
            dry_run,
            discord_gateway: false,
            discord_http: true,
            discord_message_creates: false,
            discord_message_edits: !dry_run,
            discord_message_deletes: false,
            discord_role_modifications: false,
            discord_dms: false,
            google_sheets: false,
            legacy_json_writes: false,
            legacy_db_writes: false,
            migrations: false,
            allowed_mentions_disabled: true,
        }
    }

    pub fn no_discord() -> Self {
        Self {
            dry_run: true,
            discord_gateway: false,
            discord_http: false,
            discord_message_creates: false,
            discord_message_edits: false,
            discord_message_deletes: false,
            discord_role_modifications: false,
            discord_dms: false,
            google_sheets: false,
            legacy_json_writes: false,
            legacy_db_writes: false,
            migrations: false,
            allowed_mentions_disabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateMessagePayload {
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub message_id: u64,
    pub content: Option<String>,
    pub allowed_mentions_disabled: bool,
    pub embeds: Vec<BootstrapEmbedPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateOperation {
    pub operation_type: &'static str,
    pub panel_name: &'static str,
    pub channel_id: u64,
    pub message_id: u64,
    pub allowed: bool,
    pub status: String,
    pub expected_title: String,
    pub expected_embed_count: usize,
    pub edited_message_id: Option<u64>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateOperationOutcome {
    pub panel_name: &'static str,
    pub edited_message_id: Option<u64>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistUpdatePanelsModel {
    pub mode: &'static str,
    pub safety: UpdateSafety,
    pub dry_run: bool,
    pub state_file_path: String,
    pub guild_id: u64,
    pub bot_user_id: u64,
    pub render_summary: BootstrapRenderSummary,
    pub target_checks: Vec<TargetMessageCheck>,
    pub payloads: Vec<UpdateMessagePayload>,
    pub operations: Vec<UpdateOperation>,
    pub state_updated_path: Option<String>,
    pub partial_recovery_file_path: Option<String>,
    pub manual_next_steps: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistUpdatePanelsOutput {
    pub mode: &'static str,
    pub safety: UpdateSafety,
    pub dry_run: bool,
    pub state_file_path: Option<String>,
    pub guild_id: Option<u64>,
    pub bot_user_id: Option<u64>,
    pub render_summary: Option<BootstrapRenderSummary>,
    pub target_checks: Vec<TargetMessageCheck>,
    pub payloads: Vec<UpdateMessagePayload>,
    pub operations: Vec<UpdateOperation>,
    pub state_updated_path: Option<String>,
    pub partial_recovery_file_path: Option<String>,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
    pub manual_next_steps: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClanlistUpdatePanelsResult {
    pub report: Report,
    pub model: Option<ClanlistUpdatePanelsModel>,
    pub safety: UpdateSafety,
}

impl ClanlistUpdatePanelsResult {
    pub fn no_discord(report: Report) -> Self {
        Self {
            report,
            model: None,
            safety: UpdateSafety::no_discord(),
        }
    }

    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

pub fn build_preview(load: &ConfigLoad, options: ClanlistPreviewOptions) -> ClanlistPreviewResult {
    let mut report = Report::new();
    merge_relevant_config_status(load, &mut report);
    validate_required_env(load, &mut report);

    let config = &load.config;
    let data_dir = &config.legacy_paths.clanlist_data_dir.resolved;
    if !data_dir.is_dir() {
        report.fail(
            "clanlist",
            format!(
                "LEGACY_CLANLIST_DATA_DIR missing: {}",
                config.legacy_paths.clanlist_data_dir.display()
            ),
        );
        return ClanlistPreviewResult {
            report,
            model: None,
        };
    }
    report.ok(
        "clanlist",
        format!(
            "data dir exists: {}",
            config.legacy_paths.clanlist_data_dir.display()
        ),
    );

    let main_message_id = read_required_message_id(
        data_dir,
        "main_roster_message_ids.json",
        "main roster",
        &mut report,
    );
    let admin_message_id = read_required_message_id(
        data_dir,
        "admin_roster_message_ids.json",
        "admin roster",
        &mut report,
    );
    let steam_message_id = read_required_message_id(
        data_dir,
        "steam_roster_message_ids.json",
        "steam roster",
        &mut report,
    );

    let mut steam_cache_count = 0;
    let mut steam_records = Vec::new();
    match options.steam {
        SteamPreviewMode::Disabled => {
            report.ok("clanlist", "Steam cache preview disabled by --no-steam");
        }
        SteamPreviewMode::Auto | SteamPreviewMode::Include => {
            let cache_path = data_dir.join("steam_roster_cache.json");
            if !cache_path.is_file() {
                let message = format!("steam cache missing: {}", cache_path.display());
                if options.steam == SteamPreviewMode::Include {
                    report.fail("clanlist", message);
                } else {
                    report.warn("clanlist", message);
                }
            } else {
                match fs::read_to_string(&cache_path) {
                    Ok(text) => match parse_steam_cache(&text) {
                        Ok(records) => {
                            steam_cache_count = records.len();
                            steam_records = records;
                            report.ok(
                                "clanlist",
                                format!("steam cache records = {steam_cache_count}"),
                            );
                        }
                        Err(message) => report.fail(
                            "clanlist",
                            format!("invalid JSON in {}: {message}", cache_path.display()),
                        ),
                    },
                    Err(err) => report.fail(
                        "clanlist",
                        format!("failed to read {}: {err}", cache_path.display()),
                    ),
                }
            }
        }
    }

    let model = match (main_message_id, admin_message_id, steam_message_id) {
        (Some(main_message_id), Some(admin_message_id), Some(steam_message_id))
            if !report.has_failures() =>
        {
            Some(ClanlistPreviewModel {
                mode: OFFLINE_MODE,
                safety: PreviewSafety::offline_read_only(),
                targets: ClanlistPreviewTargets {
                    main: ClanlistRosterTarget {
                        channel_id: config.clanlist.main_channel_id,
                        message_id: main_message_id,
                        role_order: config.clanlist.main_role_ids.clone(),
                    },
                    admin: ClanlistRosterTarget {
                        channel_id: config.clanlist.admin_channel_id,
                        message_id: admin_message_id,
                        role_order: config.clanlist.admin_role_ids.clone(),
                    },
                    steam: ClanlistSteamTarget {
                        channel_id: config.clanlist.steam_channel_id,
                        message_id: steam_message_id,
                        active_role_id: config.clanlist.steam_active_role_id,
                        cached_records: steam_cache_count,
                    },
                },
                steam_cache_count,
                steam_records,
                note: OFFLINE_NOTE,
            })
        }
        _ => None,
    };

    ClanlistPreviewResult { report, model }
}

pub fn build_discord_readonly_snapshot(
    preview: ClanlistPreviewResult,
    guild_id: u64,
    roles: Vec<DiscordRoleSnapshotInput>,
    members: Option<Vec<DiscordMemberSnapshotInput>>,
    include_members: bool,
    discord_report: Report,
) -> ClanlistDiscordSnapshotResult {
    let mut report = preview.report;
    report.extend(discord_report);

    let Some(preview_model) = preview.model else {
        return ClanlistDiscordSnapshotResult {
            report,
            model: None,
            safety: PreviewSafety::discord_http_read_only(),
        };
    };

    let role_map: BTreeMap<u64, String> = roles
        .iter()
        .map(|role| (role.id, role.name.clone()))
        .collect();
    let configured_role_ids = configured_role_set(
        &preview_model.targets.main.role_order,
        &preview_model.targets.admin.role_order,
    );
    let missing_configured_roles: Vec<u64> = configured_role_ids
        .iter()
        .copied()
        .filter(|role_id| !role_map.contains_key(role_id))
        .collect();

    for role_id in &missing_configured_roles {
        report.warn(
            "clanlist",
            format!("configured roster role missing in guild: {role_id}"),
        );
    }
    if !configured_role_ids.is_empty()
        && missing_configured_roles.len() == configured_role_ids.len()
    {
        report.fail(
            "clanlist",
            "all configured roster roles are missing in guild",
        );
    }

    if include_members && members.is_none() {
        report.fail(
            "discord",
            "member fetch was requested but no member snapshot was provided",
        );
    }

    let member_refs = members.as_deref();
    let main_roles = build_roster_roles(
        &preview_model.targets.main.role_order,
        &role_map,
        member_refs,
    );
    let admin_roles = build_roster_roles(
        &preview_model.targets.admin.role_order,
        &role_map,
        member_refs,
    );

    let members_with_multiple_configured_main_roles = member_refs
        .map(|members| multiple_role_matches(members, &preview_model.targets.main.role_order))
        .unwrap_or_default();
    let members_with_multiple_configured_admin_roles = member_refs
        .map(|members| multiple_role_matches(members, &preview_model.targets.admin.role_order))
        .unwrap_or_default();
    let all_configured_roles: Vec<u64> = configured_role_ids.iter().copied().collect();
    let members_with_multiple_configured_roster_roles = member_refs
        .map(|members| multiple_role_matches(members, &all_configured_roles))
        .unwrap_or_default();
    let members_with_none_of_configured_roster_roles = member_refs
        .map(|members| members_without_configured_roles(members, &configured_role_ids))
        .unwrap_or_default();

    for member in &members_with_multiple_configured_main_roles {
        report.warn(
            "clanlist",
            format!(
                "member appears in multiple configured main roles: user_id={} role_ids={}",
                member.user_id,
                join_ids(&member.matching_role_ids)
            ),
        );
    }
    for member in &members_with_multiple_configured_admin_roles {
        report.warn(
            "clanlist",
            format!(
                "member appears in multiple configured admin roles: user_id={} role_ids={}",
                member.user_id,
                join_ids(&member.matching_role_ids)
            ),
        );
    }

    let model = ClanlistDiscordSnapshotModel {
        mode: DISCORD_SNAPSHOT_MODE,
        safety: PreviewSafety::discord_http_read_only(),
        guild_id,
        targets: preview_model.targets,
        role_count: roles.len(),
        member_count: members.as_ref().map(Vec::len),
        configured_main_role_ids: main_roles.iter().map(|role| role.role_id).collect(),
        configured_admin_role_ids: admin_roles.iter().map(|role| role.role_id).collect(),
        missing_configured_roles,
        main_roles,
        admin_roles,
        members_with_multiple_configured_main_roles,
        members_with_multiple_configured_admin_roles,
        members_with_multiple_configured_roster_roles,
        members_with_none_of_configured_roster_roles,
        note: DISCORD_SNAPSHOT_NOTE,
    };

    ClanlistDiscordSnapshotResult {
        report,
        model: Some(model),
        safety: PreviewSafety::discord_http_read_only(),
    }
}

pub fn build_render_preview(
    preview: ClanlistPreviewResult,
    guild_id: u64,
    roles: Vec<DiscordRoleSnapshotInput>,
    members: Option<Vec<DiscordMemberSnapshotInput>>,
    include_members: bool,
    discord_report: Report,
) -> ClanlistRenderPreviewResult {
    let mut report = preview.report;
    report.extend(discord_report);

    let Some(preview_model) = preview.model else {
        return ClanlistRenderPreviewResult {
            report,
            model: None,
            safety: PreviewSafety::discord_http_read_only(),
        };
    };

    if include_members && members.is_none() {
        report.fail(
            "discord",
            "member fetch was requested but no member snapshot was provided",
        );
    }

    let role_map: BTreeMap<u64, String> = roles
        .iter()
        .map(|role| (role.id, role.name.clone()))
        .collect();
    let member_refs = members.as_deref();

    let main_panel = build_roster_render_panel(
        "main",
        MAIN_PANEL_TITLE,
        MAIN_PANEL_MARKER_URL,
        RenderTarget {
            channel_id: preview_model.targets.main.channel_id,
            message_id: preview_model.targets.main.message_id,
        },
        &preview_model.targets.main.role_order,
        &role_map,
        member_refs,
        &mut report,
    );
    let admin_panel = build_roster_render_panel(
        "admin",
        ADMIN_PANEL_TITLE,
        ADMIN_PANEL_MARKER_URL,
        RenderTarget {
            channel_id: preview_model.targets.admin.channel_id,
            message_id: preview_model.targets.admin.message_id,
        },
        &preview_model.targets.admin.role_order,
        &role_map,
        member_refs,
        &mut report,
    );

    if let Some(members) = member_refs {
        warn_multiple_panel_roles(
            "main",
            members,
            &preview_model.targets.main.role_order,
            &mut report,
        );
        warn_multiple_panel_roles(
            "admin",
            members,
            &preview_model.targets.admin.role_order,
            &mut report,
        );
    } else {
        report.warn(
            "clanlist",
            "members skipped by --roles-only; roster member render preview is incomplete",
        );
    }

    let steam_panel = if preview_model.steam_records.is_empty() {
        None
    } else {
        Some(build_steam_render_panel(
            RenderTarget {
                channel_id: preview_model.targets.steam.channel_id,
                message_id: preview_model.targets.steam.message_id,
            },
            preview_model.targets.steam.active_role_id,
            &preview_model.steam_records,
            member_refs,
            &mut report,
        ))
    };

    report.warn(
        "clanlist",
        "render preview cannot verify exact live Discord message layout because it does not fetch, send, or edit panel messages",
    );

    let mut limitations = vec![
        "Google Sheets is disabled; Steam preview uses legacy steam_roster_cache.json only."
            .to_owned(),
        "Legacy JSON/cache files are read-only; no Steam cache status/display-name updates are written."
            .to_owned(),
        "Discord message edits, sends, pins, placeholders, and allowed_mentions behavior are not executed."
            .to_owned(),
        "Footer timestamps are represented as the legacy format template rather than a live render timestamp."
            .to_owned(),
    ];
    if !include_members {
        limitations.push(
            "Member fetch was skipped by --roles-only, so roster and Steam active/excluded member lists are incomplete."
                .to_owned(),
        );
    }
    if steam_panel.is_none() {
        limitations
            .push("Steam panel render is omitted because no cache records were loaded.".to_owned());
    }

    let model = ClanlistRenderPreviewModel {
        mode: RENDER_PREVIEW_MODE,
        safety: PreviewSafety::discord_http_read_only(),
        guild_id,
        role_count: roles.len(),
        member_count: members.as_ref().map(Vec::len),
        main_panel,
        admin_panel,
        steam_panel,
        limitations,
    };

    ClanlistRenderPreviewResult {
        report,
        model: Some(model),
        safety: PreviewSafety::discord_http_read_only(),
    }
}

pub fn build_write_plan(
    render: ClanlistRenderPreviewResult,
    service_guard_report: Report,
) -> ClanlistWritePlanResult {
    let mut report = render.report;
    report.extend(service_guard_report);

    let Some(render_model) = render.model else {
        return ClanlistWritePlanResult {
            report,
            model: None,
            safety: PreviewSafety::discord_http_read_only(),
        };
    };

    let mut checks = Vec::new();
    add_plan_check(
        &mut report,
        &mut checks,
        xiii_core::Severity::Ok,
        "render_preview_built",
        "render preview model was built before write planning",
    );

    let operations = vec![
        roster_write_operation(&render_model.main_panel),
        roster_write_operation(&render_model.admin_panel),
        steam_write_operation(render_model.steam_panel.as_ref()),
    ];

    add_plan_check(
        &mut report,
        &mut checks,
        if operations.len() == 3 {
            xiii_core::Severity::Ok
        } else {
            xiii_core::Severity::Fail
        },
        "exactly_three_operations",
        format!("planned operations = {}", operations.len()),
    );

    for operation in &operations {
        add_plan_check(
            &mut report,
            &mut checks,
            if operation.message_id == 0 {
                xiii_core::Severity::Fail
            } else {
                xiii_core::Severity::Ok
            },
            format!("target_{}_message_id_present", operation.panel_name),
            format!(
                "target {} message id = {}",
                operation.panel_name, operation.message_id
            ),
        );
        add_plan_check(
            &mut report,
            &mut checks,
            if operation.channel_id == 0 {
                xiii_core::Severity::Fail
            } else {
                xiii_core::Severity::Ok
            },
            format!("target_{}_channel_id_present", operation.panel_name),
            format!(
                "target {} channel id = {}",
                operation.panel_name, operation.channel_id
            ),
        );
        add_plan_check(
            &mut report,
            &mut checks,
            if operation.allowed {
                xiii_core::Severity::Fail
            } else {
                xiii_core::Severity::Ok
            },
            format!("target_{}_operation_disallowed", operation.panel_name),
            "operation allowed=false in dry-run write plan only",
        );
    }

    let mut message_ids = operations
        .iter()
        .map(|operation| operation.message_id)
        .collect::<Vec<_>>();
    message_ids.sort_unstable();
    let duplicate_message_ids = message_ids
        .windows(2)
        .filter_map(|window| (window[0] == window[1]).then_some(window[0]))
        .collect::<BTreeSet<_>>();
    add_plan_check(
        &mut report,
        &mut checks,
        if duplicate_message_ids.is_empty() {
            xiii_core::Severity::Ok
        } else {
            xiii_core::Severity::Fail
        },
        "no_duplicate_target_message_ids",
        if duplicate_message_ids.is_empty() {
            "target message IDs are unique".to_owned()
        } else {
            format!(
                "duplicate target message IDs: {}",
                join_ids(&duplicate_message_ids.iter().copied().collect::<Vec<_>>())
            )
        },
    );

    for (name, detail) in [
        (
            "google_sheets_disabled",
            "write plan uses legacy Steam cache only and does not call Google Sheets",
        ),
        (
            "legacy_json_writes_disabled",
            "legacy JSON files are read-only inputs and are not write targets",
        ),
        (
            "discord_writes_disabled",
            "planned Discord operations are allowed=false and are not executed",
        ),
        (
            "write_state_allowed_false",
            "module manifest remains write_state_allowed=false for this milestone",
        ),
        (
            "no_filesystem_legacy_output_target",
            "planned write targets are Discord channel/message IDs, not legacy filesystem paths",
        ),
    ] {
        add_plan_check(
            &mut report,
            &mut checks,
            xiii_core::Severity::Ok,
            name,
            detail,
        );
    }

    for operation in &operations {
        if operation.expected_message_chunk_count > 1 {
            add_plan_check(
                &mut report,
                &mut checks,
                xiii_core::Severity::Warn,
                format!("target_{}_requires_multiple_messages", operation.panel_name),
                format!(
                    "{} would need {} message chunks at Discord's 10-embed message limit",
                    operation.panel_name, operation.expected_message_chunk_count
                ),
            );
        }
    }

    let render_summary = WritePlanRenderSummary {
        guild_id: render_model.guild_id,
        role_count: render_model.role_count,
        member_count: render_model.member_count,
        main_total_members: render_model.main_panel.total_members,
        admin_total_members: render_model.admin_panel.total_members,
        steam_active_records: render_model
            .steam_panel
            .as_ref()
            .and_then(|panel| panel.active_count),
        steam_excluded_records: render_model
            .steam_panel
            .as_ref()
            .and_then(|panel| panel.excluded_count),
        steam_unknown_member_records: render_model
            .steam_panel
            .as_ref()
            .map(|panel| panel.unknown_member_count),
    };
    let rollback_notes = vec![
        "Do not execute this plan until the old xiii-clanlist service is stopped and verified."
            .to_owned(),
        "If a future write cutover fails, stop the superbot writer and restart the old xiii-clanlist.service."
            .to_owned(),
        "If legacy JSON is ever changed in a later milestone, restore it from backup before returning control to the old service."
            .to_owned(),
    ];
    let limitations = vec![
        "This command does not fetch current Discord message contents; message existence/layout verification is a future read-only milestone."
            .to_owned(),
        "This command never sends, edits, deletes, pins, or creates Discord messages.".to_owned(),
        "This command never writes legacy JSON/cache files and never calls Google Sheets.".to_owned(),
    ];

    let model = ClanlistWritePlanModel {
        mode: WRITE_PLAN_MODE,
        safety: PreviewSafety::discord_http_read_only(),
        render_summary,
        planned_operations: operations,
        checks,
        rollback_notes,
        limitations,
    };

    ClanlistWritePlanResult {
        report,
        model: Some(model),
        safety: PreviewSafety::discord_http_read_only(),
    }
}

pub fn target_message_targets_from_preview(
    model: &ClanlistPreviewModel,
) -> Vec<TargetMessageTarget> {
    vec![
        TargetMessageTarget {
            panel_name: "main",
            channel_id: model.targets.main.channel_id,
            message_id: model.targets.main.message_id,
            expected_title: MAIN_PANEL_TITLE.to_owned(),
            expected_marker_url: MAIN_PANEL_MARKER_URL,
        },
        TargetMessageTarget {
            panel_name: "admin",
            channel_id: model.targets.admin.channel_id,
            message_id: model.targets.admin.message_id,
            expected_title: ADMIN_PANEL_TITLE.to_owned(),
            expected_marker_url: ADMIN_PANEL_MARKER_URL,
        },
        TargetMessageTarget {
            panel_name: "steam",
            channel_id: model.targets.steam.channel_id,
            message_id: model.targets.steam.message_id,
            expected_title: STEAM_PANEL_TITLE.to_owned(),
            expected_marker_url: STEAM_PANEL_MARKER_URL,
        },
    ]
}

pub fn build_target_message_check(
    preview: ClanlistPreviewResult,
    current_bot_user_id: u64,
    observations: Vec<TargetMessageObservationInput>,
    discord_report: Report,
) -> ClanlistTargetMessageCheckResult {
    let mut report = preview.report;
    report.extend(discord_report);

    let Some(preview_model) = preview.model else {
        return ClanlistTargetMessageCheckResult {
            report,
            model: None,
            safety: PreviewSafety::discord_http_read_only(),
        };
    };

    report.ok(
        "discord",
        format!("current bot user id = {current_bot_user_id}"),
    );

    let mut target_checks = Vec::new();
    for target in target_message_targets_from_preview(&preview_model) {
        target_checks.push(build_single_target_message_check(
            &target,
            current_bot_user_id,
            observations.iter().find(|observation| {
                observation.panel_name == target.panel_name
                    && observation.channel_id == target.channel_id
                    && observation.message_id == target.message_id
            }),
            &mut report,
        ));
    }

    let all_targets_editable_candidates = target_checks.len() == 3
        && target_checks
            .iter()
            .all(|check| check.message_exists && check.editable_by_current_bot);
    if all_targets_editable_candidates {
        report.ok(
            "clanlist",
            "all 3 target messages exist and are authored by the current bot; editable candidates only",
        );
    }

    let limitations = vec![
        "This command fetches only the three exact target messages; it does not scan channel history."
            .to_owned(),
        "Discord writes, message edits, sends, deletes, pins, and command registration remain disabled."
            .to_owned(),
        "Google Sheets and legacy JSON/cache writes remain disabled.".to_owned(),
    ];

    let model = ClanlistTargetMessageCheckModel {
        mode: TARGET_MESSAGE_CHECK_MODE,
        safety: PreviewSafety::discord_http_read_only(),
        current_bot_user_id,
        target_checks,
        all_targets_editable_candidates,
        limitations,
    };

    ClanlistTargetMessageCheckResult {
        report,
        model: Some(model),
        safety: PreviewSafety::discord_http_read_only(),
    }
}

fn build_single_target_message_check(
    target: &TargetMessageTarget,
    current_bot_user_id: u64,
    observation: Option<&TargetMessageObservationInput>,
    report: &mut Report,
) -> TargetMessageCheck {
    if target.channel_id == 0 {
        report.fail(
            "clanlist",
            format!("target {} channel id is zero/empty", target.panel_name),
        );
    }
    if target.message_id == 0 {
        report.fail(
            "clanlist",
            format!("target {} message id is zero/empty", target.panel_name),
        );
    }

    let Some(observation) = observation else {
        let reason = "target message was not fetched; no Discord observation was provided";
        report.fail(
            "discord",
            format!(
                "target {} message missing observation: {reason}",
                target.panel_name
            ),
        );
        return TargetMessageCheck {
            panel_name: target.panel_name,
            channel_id: target.channel_id,
            message_id: target.message_id,
            message_exists: false,
            current_bot_user_id,
            message_author_id: None,
            is_current_bot_author: None,
            editable_by_current_bot: false,
            embed_count: None,
            expected_title: target.expected_title.clone(),
            actual_first_embed_title: None,
            title_roughly_matches: None,
            first_embed_footer_text: None,
            first_embed_footer_icon_url: None,
            first_embed_marker_url: None,
            expected_marker_url: target.expected_marker_url,
            status: "FAIL".to_owned(),
            failure_reason: Some(reason.to_owned()),
        };
    };

    if !observation.exists {
        let reason = observation.failure_reason.clone().unwrap_or_else(|| {
            "message does not exist or is not accessible to the current bot".to_owned()
        });
        report.fail(
            "discord",
            format!(
                "target {} message exists = false; {reason}",
                target.panel_name
            ),
        );
        return TargetMessageCheck {
            panel_name: target.panel_name,
            channel_id: target.channel_id,
            message_id: target.message_id,
            message_exists: false,
            current_bot_user_id,
            message_author_id: observation.author_id,
            is_current_bot_author: None,
            editable_by_current_bot: false,
            embed_count: observation.embed_count,
            expected_title: target.expected_title.clone(),
            actual_first_embed_title: observation.first_embed_title.clone(),
            title_roughly_matches: None,
            first_embed_footer_text: observation.first_embed_footer_text.clone(),
            first_embed_footer_icon_url: observation.first_embed_footer_icon_url.clone(),
            first_embed_marker_url: observation.first_embed_marker_url.clone(),
            expected_marker_url: target.expected_marker_url,
            status: "FAIL".to_owned(),
            failure_reason: Some(reason),
        };
    }

    report.ok(
        "discord",
        format!("target {} message exists", target.panel_name),
    );

    let mut status = "OK".to_owned();
    let mut failure_reason = None;
    let is_current_bot_author = observation
        .author_id
        .map(|author_id| author_id == current_bot_user_id);
    let editable_by_current_bot = is_current_bot_author.unwrap_or(false);

    match observation.author_id {
        Some(author_id) => {
            report.ok(
                "discord",
                format!("target {} author id = {author_id}", target.panel_name),
            );
            if editable_by_current_bot {
                report.ok(
                    "discord",
                    format!(
                        "target {} editable_by_current_bot = true",
                        target.panel_name
                    ),
                );
            } else {
                status = "FAIL".to_owned();
                let reason = "Discord bots can edit only their own messages.".to_owned();
                report.fail(
                    "discord",
                    format!(
                        "target {} editable_by_current_bot = false; message_author_id={author_id} current_bot_id={current_bot_user_id}; reason: {reason}",
                        target.panel_name
                    ),
                );
                failure_reason = Some(reason);
            }
        }
        None => {
            status = "FAIL".to_owned();
            let reason = "message author ID was not present in Discord response".to_owned();
            report.fail(
                "discord",
                format!("target {} author id missing", target.panel_name),
            );
            failure_reason = Some(reason);
        }
    }

    if let Some(embed_count) = observation.embed_count {
        report.ok(
            "discord",
            format!("target {} embed count = {embed_count}", target.panel_name),
        );
    }

    let title_roughly_matches = observation
        .first_embed_title
        .as_deref()
        .map(|actual| titles_roughly_match(&target.expected_title, actual));
    match (&observation.first_embed_title, title_roughly_matches) {
        (Some(title), Some(true)) => report.ok(
            "discord",
            format!("target {} first_embed_title = {title}", target.panel_name),
        ),
        (Some(title), Some(false)) => report.warn(
            "discord",
            format!(
                "target {} first embed title differs; expected={} actual={title}",
                target.panel_name, target.expected_title
            ),
        ),
        (None, _) => report.warn(
            "discord",
            format!("target {} has no first embed title", target.panel_name),
        ),
        _ => {}
    }

    TargetMessageCheck {
        panel_name: target.panel_name,
        channel_id: target.channel_id,
        message_id: target.message_id,
        message_exists: true,
        current_bot_user_id,
        message_author_id: observation.author_id,
        is_current_bot_author,
        editable_by_current_bot,
        embed_count: observation.embed_count,
        expected_title: target.expected_title.clone(),
        actual_first_embed_title: observation.first_embed_title.clone(),
        title_roughly_matches,
        first_embed_footer_text: observation.first_embed_footer_text.clone(),
        first_embed_footer_icon_url: observation.first_embed_footer_icon_url.clone(),
        first_embed_marker_url: observation.first_embed_marker_url.clone(),
        expected_marker_url: target.expected_marker_url,
        status,
        failure_reason,
    }
}

fn titles_roughly_match(expected: &str, actual: &str) -> bool {
    normalize_title(expected) == normalize_title(actual)
}

fn normalize_title(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn build_bootstrap_new_panels(
    render: ClanlistRenderPreviewResult,
    preview_targets: Option<ClanlistPreviewTargets>,
    bot_user_id: u64,
    dry_run: bool,
    footer_timestamp_utc: &str,
) -> ClanlistBootstrapNewPanelsResult {
    let mut report = render.report;
    let safety = BootstrapSafety::new(dry_run);

    let Some(render_model) = render.model else {
        return ClanlistBootstrapNewPanelsResult {
            report,
            model: None,
            safety,
        };
    };

    let Some(preview_targets) = preview_targets else {
        report.fail(
            "clanlist",
            "legacy Clanlist targets are unavailable; cannot bootstrap exactly 3 new panels",
        );
        return ClanlistBootstrapNewPanelsResult {
            report,
            model: None,
            safety,
        };
    };

    report.ok("bootstrap", "render preview built for new panel bootstrap");

    let payloads = bootstrap_payloads_from_render(
        &render_model,
        &preview_targets,
        footer_timestamp_utc,
        &mut report,
    );
    validate_bootstrap_payloads(&payloads, &mut report);

    let operations = payloads
        .iter()
        .map(|payload| BootstrapOperation {
            operation_type: "create_message",
            panel_name: payload.panel_name,
            channel_id: payload.channel_id,
            legacy_message_id: payload.legacy_message_id,
            new_message_id: None,
            allowed: !dry_run,
            status: if dry_run { "dry_run" } else { "pending" }.to_owned(),
            expected_title: payload
                .embeds
                .first()
                .map(|embed| embed.title.clone())
                .unwrap_or_default(),
            expected_embed_count: payload.embeds.len(),
            failure_reason: None,
        })
        .collect::<Vec<_>>();

    let old_legacy_message_ids = BootstrapLegacyMessageIds {
        main: preview_targets.main.message_id,
        admin: preview_targets.admin.message_id,
        steam: preview_targets.steam.message_id,
    };
    let render_summary = BootstrapRenderSummary {
        guild_id: render_model.guild_id,
        role_count: render_model.role_count,
        member_count: render_model.member_count,
        main_total_members: render_model.main_panel.total_members,
        admin_total_members: render_model.admin_panel.total_members,
        steam_active_records: render_model
            .steam_panel
            .as_ref()
            .and_then(|panel| panel.active_count),
        steam_excluded_records: render_model
            .steam_panel
            .as_ref()
            .and_then(|panel| panel.excluded_count),
        steam_unknown_member_records: render_model
            .steam_panel
            .as_ref()
            .map(|panel| panel.unknown_member_count),
    };

    let manual_next_steps = vec![
        "Verify the three new Clanlist messages in Discord before doing anything to old panels."
            .to_owned(),
        "Delete old Clanlist panels manually only after the new panels are verified.".to_owned(),
        "Keep the old legacy Clanlist JSON files unchanged as backup/reference.".to_owned(),
    ];
    let limitations = vec![
        "This bootstrap uses Discord role/member reads plus legacy steam_roster_cache.json only; Google Sheets remains disabled."
            .to_owned(),
        "This command creates new messages only; it never edits or deletes old Clanlist panel messages."
            .to_owned(),
        "No scheduler or production runtime is started by this command.".to_owned(),
    ];

    let model = ClanlistBootstrapNewPanelsModel {
        mode: BOOTSTRAP_NEW_PANELS_MODE,
        safety: safety.clone(),
        dry_run,
        guild_id: render_model.guild_id,
        bot_user_id,
        render_summary,
        old_legacy_message_ids,
        payloads,
        operations,
        state_file_path: None,
        partial_recovery_file_path: None,
        manual_next_steps,
        limitations,
    };

    ClanlistBootstrapNewPanelsResult {
        report,
        model: Some(model),
        safety,
    }
}

pub fn validate_bootstrap_payloads(payloads: &[BootstrapMessagePayload], report: &mut Report) {
    if payloads.len() == 3 {
        report.ok("bootstrap", "exactly 3 create_message payloads prepared");
    } else {
        report.fail(
            "bootstrap",
            format!(
                "expected exactly 3 create_message payloads, got {}",
                payloads.len()
            ),
        );
    }

    for payload in payloads {
        if payload.channel_id == 0 {
            report.fail(
                "bootstrap",
                format!("{} target channel_id is zero/empty", payload.panel_name),
            );
        } else {
            report.ok(
                "bootstrap",
                format!(
                    "{} target channel_id = {}",
                    payload.panel_name, payload.channel_id
                ),
            );
        }

        if payload.content.is_none() {
            report.ok(
                "bootstrap",
                format!("{} payload has no normal text content", payload.panel_name),
            );
        } else {
            report.fail(
                "bootstrap",
                format!(
                    "{} payload must not use normal text content",
                    payload.panel_name
                ),
            );
        }

        if payload.allowed_mentions_disabled {
            report.ok(
                "bootstrap",
                format!("{} payload allowed_mentions disabled", payload.panel_name),
            );
        } else {
            report.fail(
                "bootstrap",
                format!(
                    "{} payload allowed_mentions must be disabled",
                    payload.panel_name
                ),
            );
        }

        if payload.embeds.is_empty() {
            report.fail(
                "bootstrap",
                format!("{} payload has no embeds", payload.panel_name),
            );
        } else if payload.embeds.len() > MAX_EMBEDS_PER_MESSAGE {
            report.fail(
                "bootstrap",
                format!(
                    "{} payload has {} embeds; Discord message limit is {}",
                    payload.panel_name,
                    payload.embeds.len(),
                    MAX_EMBEDS_PER_MESSAGE
                ),
            );
        } else {
            report.ok(
                "bootstrap",
                format!(
                    "{} payload embeds = {}",
                    payload.panel_name,
                    payload.embeds.len()
                ),
            );
        }

        for (index, embed) in payload.embeds.iter().enumerate() {
            if embed.title.trim().is_empty() {
                report.fail(
                    "bootstrap",
                    format!("{} embed {} title is empty", payload.panel_name, index + 1),
                );
            }
            if embed.description.trim().is_empty() {
                report.fail(
                    "bootstrap",
                    format!(
                        "{} embed {} description is empty",
                        payload.panel_name,
                        index + 1
                    ),
                );
            }
        }
    }
}

pub fn apply_bootstrap_outcomes(
    result: &mut ClanlistBootstrapNewPanelsResult,
    outcomes: Vec<BootstrapOperationOutcome>,
) {
    let Some(model) = result.model.as_mut() else {
        return;
    };

    for outcome in outcomes {
        if let Some(operation) = model
            .operations
            .iter_mut()
            .find(|operation| operation.panel_name == outcome.panel_name)
        {
            operation.new_message_id = outcome.new_message_id;
            operation.failure_reason = outcome.failure_reason.clone();
            match (&outcome.new_message_id, &outcome.failure_reason) {
                (Some(message_id), None) => {
                    operation.status = "created".to_owned();
                    result.report.ok(
                        "discord",
                        format!(
                            "created {} panel message id = {}",
                            operation.panel_name, message_id
                        ),
                    );
                }
                (_, Some(reason)) => {
                    operation.status = "failed".to_owned();
                    result.report.fail(
                        "discord",
                        format!(
                            "failed to create {} panel message: {reason}",
                            operation.panel_name
                        ),
                    );
                }
                (None, None) => {
                    operation.status = "skipped".to_owned();
                    result.report.warn(
                        "discord",
                        format!("{} panel message creation skipped", operation.panel_name),
                    );
                }
            }
        }
    }
}

pub fn set_bootstrap_state_file_path(
    result: &mut ClanlistBootstrapNewPanelsResult,
    path: impl Into<String>,
) {
    if let Some(model) = result.model.as_mut() {
        model.state_file_path = Some(path.into());
    }
}

pub fn set_bootstrap_partial_recovery_file_path(
    result: &mut ClanlistBootstrapNewPanelsResult,
    path: impl Into<String>,
) {
    if let Some(model) = result.model.as_mut() {
        model.partial_recovery_file_path = Some(path.into());
    }
}

pub fn build_panel_state(
    model: &ClanlistBootstrapNewPanelsModel,
    created_at_utc: &str,
) -> Result<ClanlistPanelState, String> {
    let main = created_operation(model, "main")?;
    let admin = created_operation(model, "admin")?;
    let steam = created_operation(model, "steam")?;

    Ok(ClanlistPanelState {
        created_at_utc: created_at_utc.to_owned(),
        guild_id: model.guild_id,
        bot_user_id: model.bot_user_id,
        source: "fresh_bootstrap".to_owned(),
        main: ClanlistPanelStateTarget {
            channel_id: main.channel_id,
            message_id: main
                .new_message_id
                .ok_or_else(|| "main panel was not created".to_owned())?,
        },
        admin: ClanlistPanelStateTarget {
            channel_id: admin.channel_id,
            message_id: admin
                .new_message_id
                .ok_or_else(|| "admin panel was not created".to_owned())?,
        },
        steam: ClanlistPanelStateTarget {
            channel_id: steam.channel_id,
            message_id: steam
                .new_message_id
                .ok_or_else(|| "steam panel was not created".to_owned())?,
        },
        render_summary: ClanlistPanelStateRenderSummary {
            main_total_members: model.render_summary.main_total_members,
            admin_total_members: model.render_summary.admin_total_members,
            steam_active_records: model.render_summary.steam_active_records,
            steam_excluded_records: model.render_summary.steam_excluded_records,
            steam_unknown_member_records: model.render_summary.steam_unknown_member_records,
        },
        old_legacy_message_ids: model.old_legacy_message_ids.clone(),
        warning: "Old Clanlist panels were not edited or deleted by this bootstrap.".to_owned(),
        last_updated_at_utc: None,
        last_update_source: None,
        last_run_mode: None,
        last_render_summary: None,
        last_successful_update_message_ids: None,
    })
}

fn created_operation<'a>(
    model: &'a ClanlistBootstrapNewPanelsModel,
    panel_name: &str,
) -> Result<&'a BootstrapOperation, String> {
    model
        .operations
        .iter()
        .find(|operation| operation.panel_name == panel_name && operation.status == "created")
        .ok_or_else(|| format!("{panel_name} panel was not created"))
}

fn bootstrap_payloads_from_render(
    model: &ClanlistRenderPreviewModel,
    preview_targets: &ClanlistPreviewTargets,
    footer_timestamp_utc: &str,
    report: &mut Report,
) -> Vec<BootstrapMessagePayload> {
    let mut payloads = vec![
        roster_bootstrap_payload(
            &model.main_panel,
            preview_targets.main.message_id,
            footer_timestamp_utc,
        ),
        roster_bootstrap_payload(
            &model.admin_panel,
            preview_targets.admin.message_id,
            footer_timestamp_utc,
        ),
    ];

    let steam_payload = match model.steam_panel.as_ref() {
        Some(panel) => steam_bootstrap_payload(
            panel,
            preview_targets.steam.message_id,
            footer_timestamp_utc,
        ),
        None => {
            report.warn(
                "bootstrap",
                "Steam render panel unavailable; creating empty Steam panel payload from legacy target IDs",
            );
            empty_steam_bootstrap_payload(&preview_targets.steam, footer_timestamp_utc)
        }
    };
    payloads.push(steam_payload);
    payloads
}

fn roster_bootstrap_payload(
    panel: &RenderedRosterPanel,
    legacy_message_id: u64,
    footer_timestamp_utc: &str,
) -> BootstrapMessagePayload {
    let mut embeds = vec![bootstrap_embed(
        panel.header.title,
        &panel.header.description,
        panel.header.marker_url,
        footer_timestamp_utc,
    )];
    for section in &panel.role_sections {
        for chunk in &section.embed_chunks {
            embeds.push(bootstrap_embed(
                &chunk.title,
                &chunk.description,
                panel.header.marker_url,
                footer_timestamp_utc,
            ));
        }
    }

    BootstrapMessagePayload {
        panel_name: panel.panel_name,
        channel_id: panel.target.channel_id,
        legacy_message_id,
        content: None,
        allowed_mentions_disabled: true,
        embeds,
    }
}

fn steam_bootstrap_payload(
    panel: &RenderedSteamPanel,
    legacy_message_id: u64,
    footer_timestamp_utc: &str,
) -> BootstrapMessagePayload {
    let mut embeds = vec![bootstrap_embed(
        panel.header.title,
        &panel.header.description,
        panel.header.marker_url,
        footer_timestamp_utc,
    )];
    for chunk in &panel.active_block.embed_chunks {
        embeds.push(bootstrap_embed(
            &chunk.title,
            &chunk.description,
            panel.header.marker_url,
            footer_timestamp_utc,
        ));
    }
    for chunk in &panel.excluded_block.embed_chunks {
        embeds.push(bootstrap_embed(
            &chunk.title,
            &chunk.description,
            panel.header.marker_url,
            footer_timestamp_utc,
        ));
    }

    BootstrapMessagePayload {
        panel_name: panel.panel_name,
        channel_id: panel.target.channel_id,
        legacy_message_id,
        content: None,
        allowed_mentions_disabled: true,
        embeds,
    }
}

fn empty_steam_bootstrap_payload(
    target: &ClanlistSteamTarget,
    footer_timestamp_utc: &str,
) -> BootstrapMessagePayload {
    BootstrapMessagePayload {
        panel_name: "steam",
        channel_id: target.channel_id,
        legacy_message_id: target.message_id,
        content: None,
        allowed_mentions_disabled: true,
        embeds: vec![bootstrap_embed(
            STEAM_PANEL_TITLE,
            RU_NO_RECORDS,
            STEAM_PANEL_MARKER_URL,
            footer_timestamp_utc,
        )],
    }
}

fn bootstrap_embed(
    title: &str,
    description: &str,
    marker_url: &'static str,
    footer_timestamp_utc: &str,
) -> BootstrapEmbedPayload {
    BootstrapEmbedPayload {
        title: title.to_owned(),
        description: description.to_owned(),
        footer_text: RU_UPDATED_FOOTER_TEMPLATE
            .replace("<dd.mm.yyyy HH:MM>", &format!("{footer_timestamp_utc} UTC")),
        marker_url,
        color_hex: EMBED_COLOR_HEX,
    }
}

pub fn parse_panel_state_json(text: &str) -> Result<ClanlistPanelState, String> {
    serde_json::from_str(text).map_err(|err| err.to_string())
}

pub fn validate_panel_state(
    state: &ClanlistPanelState,
    expected_guild_id: u64,
    report: &mut Report,
) {
    if state.guild_id == expected_guild_id {
        report.ok(
            "state",
            format!("state guild_id matches env XIII_GUILD_ID = {expected_guild_id}"),
        );
    } else {
        report.fail(
            "state",
            format!(
                "state guild_id {} does not match env XIII_GUILD_ID {}",
                state.guild_id, expected_guild_id
            ),
        );
    }

    if state.source == "fresh_bootstrap" {
        report.ok("state", "state source = fresh_bootstrap");
    } else if state.source.trim().is_empty() {
        report.fail("state", "state source is missing/empty");
    } else {
        report.warn(
            "state",
            format!(
                "state source is not fresh_bootstrap; continuing only because panel IDs are compatible: {}",
                state.source
            ),
        );
    }

    if state.bot_user_id == 0 {
        report.fail("state", "state bot_user_id is zero/empty");
    } else {
        report.ok(
            "state",
            format!("state bot_user_id = {}", state.bot_user_id),
        );
    }

    for (name, target) in [
        ("main", &state.main),
        ("admin", &state.admin),
        ("steam", &state.steam),
    ] {
        if target.channel_id == 0 {
            report.fail("state", format!("state {name} channel_id is zero/empty"));
        } else {
            report.ok(
                "state",
                format!("state {name} channel_id = {}", target.channel_id),
            );
        }
        if target.message_id == 0 {
            report.fail("state", format!("state {name} message_id is zero/empty"));
        } else {
            report.ok(
                "state",
                format!("state {name} message_id = {}", target.message_id),
            );
        }
    }

    let message_ids = [
        state.main.message_id,
        state.admin.message_id,
        state.steam.message_id,
    ];
    let mut sorted = message_ids;
    sorted.sort_unstable();
    let duplicates = sorted
        .windows(2)
        .filter_map(|window| (window[0] == window[1]).then_some(window[0]))
        .collect::<BTreeSet<_>>();
    if duplicates.is_empty() {
        report.ok("state", "state target message IDs are unique");
    } else {
        report.fail(
            "state",
            format!(
                "state target message IDs are duplicated: {}",
                join_ids(&duplicates.iter().copied().collect::<Vec<_>>())
            ),
        );
    }
}

pub fn update_targets_from_state(state: &ClanlistPanelState) -> Vec<TargetMessageTarget> {
    vec![
        TargetMessageTarget {
            panel_name: "main",
            channel_id: state.main.channel_id,
            message_id: state.main.message_id,
            expected_title: MAIN_PANEL_TITLE.to_owned(),
            expected_marker_url: MAIN_PANEL_MARKER_URL,
        },
        TargetMessageTarget {
            panel_name: "admin",
            channel_id: state.admin.channel_id,
            message_id: state.admin.message_id,
            expected_title: ADMIN_PANEL_TITLE.to_owned(),
            expected_marker_url: ADMIN_PANEL_MARKER_URL,
        },
        TargetMessageTarget {
            panel_name: "steam",
            channel_id: state.steam.channel_id,
            message_id: state.steam.message_id,
            expected_title: STEAM_PANEL_TITLE.to_owned(),
            expected_marker_url: STEAM_PANEL_MARKER_URL,
        },
    ]
}

pub fn build_update_panels(
    render: ClanlistRenderPreviewResult,
    state: ClanlistPanelState,
    state_file_path: impl Into<String>,
    current_bot_user_id: u64,
    observations: Vec<TargetMessageObservationInput>,
    service_guard_report: Report,
    dry_run: bool,
    footer_timestamp_utc: &str,
) -> ClanlistUpdatePanelsResult {
    let mut report = render.report;
    report.extend(service_guard_report);
    let safety = UpdateSafety::new(dry_run);

    if state.bot_user_id == current_bot_user_id {
        report.ok(
            "discord",
            format!("current bot id matches state bot_user_id = {current_bot_user_id}"),
        );
    } else {
        report.fail(
            "discord",
            format!(
                "current bot id {} does not match state bot_user_id {}",
                current_bot_user_id, state.bot_user_id
            ),
        );
    }

    let Some(render_model) = render.model else {
        return ClanlistUpdatePanelsResult {
            report,
            model: None,
            safety,
        };
    };

    let targets = update_targets_from_state(&state);
    let target_checks = targets
        .iter()
        .map(|target| {
            build_single_target_message_check(
                target,
                current_bot_user_id,
                observations.iter().find(|observation| {
                    observation.panel_name == target.panel_name
                        && observation.channel_id == target.channel_id
                        && observation.message_id == target.message_id
                }),
                &mut report,
            )
        })
        .collect::<Vec<_>>();

    let payloads =
        update_payloads_from_render(&render_model, &state, footer_timestamp_utc, &mut report);
    validate_update_payloads(&payloads, &mut report);

    let operations = payloads
        .iter()
        .map(|payload| UpdateOperation {
            operation_type: "edit_existing_message",
            panel_name: payload.panel_name,
            channel_id: payload.channel_id,
            message_id: payload.message_id,
            allowed: !dry_run,
            status: if dry_run { "dry_run" } else { "pending" }.to_owned(),
            expected_title: payload
                .embeds
                .first()
                .map(|embed| embed.title.clone())
                .unwrap_or_default(),
            expected_embed_count: payload.embeds.len(),
            edited_message_id: None,
            failure_reason: None,
        })
        .collect::<Vec<_>>();

    let render_summary = BootstrapRenderSummary {
        guild_id: render_model.guild_id,
        role_count: render_model.role_count,
        member_count: render_model.member_count,
        main_total_members: render_model.main_panel.total_members,
        admin_total_members: render_model.admin_panel.total_members,
        steam_active_records: render_model
            .steam_panel
            .as_ref()
            .and_then(|panel| panel.active_count),
        steam_excluded_records: render_model
            .steam_panel
            .as_ref()
            .and_then(|panel| panel.excluded_count),
        steam_unknown_member_records: render_model
            .steam_panel
            .as_ref()
            .map(|panel| panel.unknown_member_count),
    };

    let manual_next_steps = vec![
        "Verify the three updated Clanlist messages in Discord.".to_owned(),
        "Old legacy Clanlist panels and legacy JSON remain backup/reference only.".to_owned(),
    ];
    let limitations = vec![
        "This command updates only the three fresh_bootstrap panel messages from the Superbot state file."
            .to_owned(),
        "Google Sheets remains disabled; Steam rendering uses legacy steam_roster_cache.json only."
            .to_owned(),
        "No scheduler or production daemon/runtime is started by this command.".to_owned(),
    ];

    let model = ClanlistUpdatePanelsModel {
        mode: UPDATE_PANELS_MODE,
        safety: safety.clone(),
        dry_run,
        state_file_path: state_file_path.into(),
        guild_id: state.guild_id,
        bot_user_id: state.bot_user_id,
        render_summary,
        target_checks,
        payloads,
        operations,
        state_updated_path: None,
        partial_recovery_file_path: None,
        manual_next_steps,
        limitations,
    };

    ClanlistUpdatePanelsResult {
        report,
        model: Some(model),
        safety,
    }
}

pub fn validate_update_payloads(payloads: &[UpdateMessagePayload], report: &mut Report) {
    if payloads.len() == 3 {
        report.ok("update", "exactly 3 edit payloads prepared");
    } else {
        report.fail(
            "update",
            format!("expected exactly 3 edit payloads, got {}", payloads.len()),
        );
    }

    for payload in payloads {
        if payload.channel_id == 0 || payload.message_id == 0 {
            report.fail(
                "update",
                format!(
                    "{} payload target channel/message ID is zero/empty",
                    payload.panel_name
                ),
            );
        } else {
            report.ok(
                "update",
                format!(
                    "{} payload target = {}/{}",
                    payload.panel_name, payload.channel_id, payload.message_id
                ),
            );
        }

        if payload.content.is_none() {
            report.ok(
                "update",
                format!("{} payload has no normal text content", payload.panel_name),
            );
        } else {
            report.fail(
                "update",
                format!(
                    "{} payload must not use normal text content",
                    payload.panel_name
                ),
            );
        }

        if payload.allowed_mentions_disabled {
            report.ok(
                "update",
                format!("{} payload allowed_mentions disabled", payload.panel_name),
            );
        } else {
            report.fail(
                "update",
                format!(
                    "{} payload allowed_mentions must be disabled",
                    payload.panel_name
                ),
            );
        }

        if payload.embeds.is_empty() {
            report.fail(
                "update",
                format!("{} payload has no embeds", payload.panel_name),
            );
        } else if payload.embeds.len() > MAX_EMBEDS_PER_MESSAGE {
            report.fail(
                "update",
                format!(
                    "{} payload has {} embeds; Discord message limit is {}",
                    payload.panel_name,
                    payload.embeds.len(),
                    MAX_EMBEDS_PER_MESSAGE
                ),
            );
        } else {
            report.ok(
                "update",
                format!(
                    "{} payload embeds = {}",
                    payload.panel_name,
                    payload.embeds.len()
                ),
            );
        }

        for (index, embed) in payload.embeds.iter().enumerate() {
            if embed.title.trim().is_empty() {
                report.fail(
                    "update",
                    format!("{} embed {} title is empty", payload.panel_name, index + 1),
                );
            }
            if embed.description.trim().is_empty() {
                report.fail(
                    "update",
                    format!(
                        "{} embed {} description is empty",
                        payload.panel_name,
                        index + 1
                    ),
                );
            }
        }
    }
}

pub fn apply_update_outcomes(
    result: &mut ClanlistUpdatePanelsResult,
    outcomes: Vec<UpdateOperationOutcome>,
) {
    let Some(model) = result.model.as_mut() else {
        return;
    };

    for outcome in outcomes {
        if let Some(operation) = model
            .operations
            .iter_mut()
            .find(|operation| operation.panel_name == outcome.panel_name)
        {
            operation.edited_message_id = outcome.edited_message_id;
            operation.failure_reason = outcome.failure_reason.clone();
            match (&outcome.edited_message_id, &outcome.failure_reason) {
                (Some(message_id), None) => {
                    operation.status = "edited".to_owned();
                    result.report.ok(
                        "discord",
                        format!(
                            "edited {} panel message id = {}",
                            operation.panel_name, message_id
                        ),
                    );
                }
                (_, Some(reason)) => {
                    operation.status = "failed".to_owned();
                    result.report.fail(
                        "discord",
                        format!(
                            "failed to edit {} panel message: {reason}",
                            operation.panel_name
                        ),
                    );
                }
                (None, None) => {
                    operation.status = "skipped".to_owned();
                    result.report.warn(
                        "discord",
                        format!("{} panel message edit skipped", operation.panel_name),
                    );
                }
            }
        }
    }
}

pub fn set_update_state_updated_path(
    result: &mut ClanlistUpdatePanelsResult,
    path: impl Into<String>,
) {
    if let Some(model) = result.model.as_mut() {
        model.state_updated_path = Some(path.into());
    }
}

pub fn set_update_partial_recovery_file_path(
    result: &mut ClanlistUpdatePanelsResult,
    path: impl Into<String>,
) {
    if let Some(model) = result.model.as_mut() {
        model.partial_recovery_file_path = Some(path.into());
    }
}

pub fn apply_successful_update_to_state(
    state: &mut ClanlistPanelState,
    model: &ClanlistUpdatePanelsModel,
    last_updated_at_utc: &str,
) -> Result<(), String> {
    let main = edited_operation(model, "main")?;
    let admin = edited_operation(model, "admin")?;
    let steam = edited_operation(model, "steam")?;

    state.last_updated_at_utc = Some(last_updated_at_utc.to_owned());
    state.last_update_source = Some("manual_update_command".to_owned());
    state.last_render_summary = Some(ClanlistPanelStateRenderSummary {
        main_total_members: model.render_summary.main_total_members,
        admin_total_members: model.render_summary.admin_total_members,
        steam_active_records: model.render_summary.steam_active_records,
        steam_excluded_records: model.render_summary.steam_excluded_records,
        steam_unknown_member_records: model.render_summary.steam_unknown_member_records,
    });
    state.last_successful_update_message_ids = Some(ClanlistPanelStateMessageIds {
        main: main
            .edited_message_id
            .ok_or_else(|| "main panel was not edited".to_owned())?,
        admin: admin
            .edited_message_id
            .ok_or_else(|| "admin panel was not edited".to_owned())?,
        steam: steam
            .edited_message_id
            .ok_or_else(|| "steam panel was not edited".to_owned())?,
    });
    Ok(())
}

fn edited_operation<'a>(
    model: &'a ClanlistUpdatePanelsModel,
    panel_name: &str,
) -> Result<&'a UpdateOperation, String> {
    model
        .operations
        .iter()
        .find(|operation| operation.panel_name == panel_name && operation.status == "edited")
        .ok_or_else(|| format!("{panel_name} panel was not edited"))
}

fn update_payloads_from_render(
    model: &ClanlistRenderPreviewModel,
    state: &ClanlistPanelState,
    footer_timestamp_utc: &str,
    report: &mut Report,
) -> Vec<UpdateMessagePayload> {
    let mut payloads = vec![
        roster_update_payload(&model.main_panel, &state.main, footer_timestamp_utc),
        roster_update_payload(&model.admin_panel, &state.admin, footer_timestamp_utc),
    ];

    let steam_payload = match model.steam_panel.as_ref() {
        Some(panel) => steam_update_payload(panel, &state.steam, footer_timestamp_utc),
        None => {
            report.warn(
                "update",
                "Steam render panel unavailable; updating Steam panel with empty cache payload",
            );
            empty_steam_update_payload(&state.steam, footer_timestamp_utc)
        }
    };
    payloads.push(steam_payload);
    payloads
}

fn roster_update_payload(
    panel: &RenderedRosterPanel,
    target: &ClanlistPanelStateTarget,
    footer_timestamp_utc: &str,
) -> UpdateMessagePayload {
    let create_payload = roster_bootstrap_payload(panel, target.message_id, footer_timestamp_utc);
    UpdateMessagePayload {
        panel_name: create_payload.panel_name,
        channel_id: target.channel_id,
        message_id: target.message_id,
        content: None,
        allowed_mentions_disabled: create_payload.allowed_mentions_disabled,
        embeds: create_payload.embeds,
    }
}

fn steam_update_payload(
    panel: &RenderedSteamPanel,
    target: &ClanlistPanelStateTarget,
    footer_timestamp_utc: &str,
) -> UpdateMessagePayload {
    let create_payload = steam_bootstrap_payload(panel, target.message_id, footer_timestamp_utc);
    UpdateMessagePayload {
        panel_name: create_payload.panel_name,
        channel_id: target.channel_id,
        message_id: target.message_id,
        content: None,
        allowed_mentions_disabled: create_payload.allowed_mentions_disabled,
        embeds: create_payload.embeds,
    }
}

fn empty_steam_update_payload(
    target: &ClanlistPanelStateTarget,
    footer_timestamp_utc: &str,
) -> UpdateMessagePayload {
    let create_payload = empty_steam_bootstrap_payload(
        &ClanlistSteamTarget {
            channel_id: target.channel_id,
            message_id: target.message_id,
            active_role_id: 0,
            cached_records: 0,
        },
        footer_timestamp_utc,
    );
    UpdateMessagePayload {
        panel_name: create_payload.panel_name,
        channel_id: target.channel_id,
        message_id: target.message_id,
        content: None,
        allowed_mentions_disabled: create_payload.allowed_mentions_disabled,
        embeds: create_payload.embeds,
    }
}

pub fn render_text(result: &ClanlistPreviewResult) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Offline Preview\n");
    out.push_str("Mode: READ ONLY\n");
    out.push_str("Discord login: DISABLED\n");
    out.push_str("Discord Gateway: DISABLED\n");
    out.push_str("Discord HTTP: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Migrations: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nMain roster target:\n");
        out.push_str(&format!(
            "  channel_id: {}\n",
            model.targets.main.channel_id
        ));
        out.push_str(&format!(
            "  message_id: {}\n",
            model.targets.main.message_id
        ));
        out.push_str("  role_order:\n");
        for role_id in &model.targets.main.role_order {
            out.push_str(&format!("    - {role_id}\n"));
        }

        out.push_str("\nAdmin roster target:\n");
        out.push_str(&format!(
            "  channel_id: {}\n",
            model.targets.admin.channel_id
        ));
        out.push_str(&format!(
            "  message_id: {}\n",
            model.targets.admin.message_id
        ));
        out.push_str("  role_order:\n");
        for role_id in &model.targets.admin.role_order {
            out.push_str(&format!("    - {role_id}\n"));
        }

        out.push_str("\nSteam roster target:\n");
        out.push_str(&format!(
            "  channel_id: {}\n",
            model.targets.steam.channel_id
        ));
        out.push_str(&format!(
            "  message_id: {}\n",
            model.targets.steam.message_id
        ));
        out.push_str(&format!(
            "  active_role_id: {}\n",
            model.targets.steam.active_role_id
        ));
        out.push_str(&format!("  cached_records: {}\n", model.steam_cache_count));
        if !model.steam_records.is_empty() {
            out.push_str("  cached_record_preview:\n");
            for record in &model.steam_records {
                out.push_str(&format!(
                    "    - discord_id={} steam_id64={} status={} display_name={}\n",
                    record.discord_id,
                    record.steam_id64,
                    record.last_status.as_deref().unwrap_or("<unknown>"),
                    record.last_display_name.as_deref().unwrap_or("<unknown>")
                ));
            }
        }
    }

    let counts = result.report.counts();
    out.push_str("\nNote:\n");
    out.push_str(
        "  This is an offline preview only. It does not query Discord members or Google Sheets.\n",
    );
    out.push_str("  Main/admin roster member lists are not final without live Discord member and role data.\n");
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_json(result: &ClanlistPreviewResult) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let envelope = ClanlistPreviewOutput {
        mode: OFFLINE_MODE,
        safety: PreviewSafety::offline_read_only(),
        targets: result.model.as_ref().map(|model| model.targets.clone()),
        steam_cache_count: result.model.as_ref().map(|model| model.steam_cache_count),
        steam_records: result
            .model
            .as_ref()
            .map(|model| model.steam_records.clone())
            .unwrap_or_default(),
        warnings,
        failures,
        note: OFFLINE_NOTE,
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn render_discord_snapshot_text(result: &ClanlistDiscordSnapshotResult) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Discord Read-Only Snapshot\n");
    out.push_str("Mode: READ ONLY\n");
    out.push_str("Discord HTTP reads: ENABLED ONLY WITH --allow-discord-read\n");
    out.push_str("Discord Gateway: DISABLED\n");
    out.push_str("Discord writes: DISABLED\n");
    out.push_str("Message edits: DISABLED\n");
    out.push_str("Role modifications: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Migrations: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nGuild snapshot:\n");
        out.push_str(&format!("  guild_id: {}\n", model.guild_id));
        out.push_str(&format!("  roles_fetched: {}\n", model.role_count));
        match model.member_count {
            Some(count) => out.push_str(&format!("  members_fetched: {count}\n")),
            None => out.push_str("  members_fetched: skipped by --roles-only\n"),
        }

        out.push_str("\nLegacy message IDs:\n");
        out.push_str(&format!(
            "  main_roster_message_id: {}\n",
            model.targets.main.message_id
        ));
        out.push_str(&format!(
            "  admin_roster_message_id: {}\n",
            model.targets.admin.message_id
        ));
        out.push_str(&format!(
            "  steam_roster_message_id: {}\n",
            model.targets.steam.message_id
        ));

        out.push_str("\nMain roster configured roles:\n");
        render_role_rows(&mut out, &model.main_roles);

        out.push_str("\nAdmin roster configured roles:\n");
        render_role_rows(&mut out, &model.admin_roles);

        out.push_str("\nSteam roster target:\n");
        out.push_str(&format!(
            "  channel_id: {}\n",
            model.targets.steam.channel_id
        ));
        out.push_str(&format!(
            "  message_id: {}\n",
            model.targets.steam.message_id
        ));
        out.push_str(&format!(
            "  active_role_id: {}\n",
            model.targets.steam.active_role_id
        ));
        out.push_str(&format!(
            "  cached_records: {}\n",
            model.targets.steam.cached_records
        ));

        if !model.missing_configured_roles.is_empty() {
            out.push_str("\nMissing configured roles:\n");
            for role_id in &model.missing_configured_roles {
                out.push_str(&format!("  - {role_id}\n"));
            }
        }

        render_member_role_matches(
            &mut out,
            "Members with multiple configured main roles",
            &model.members_with_multiple_configured_main_roles,
        );
        render_member_role_matches(
            &mut out,
            "Members with multiple configured admin roles",
            &model.members_with_multiple_configured_admin_roles,
        );
        render_member_list(
            &mut out,
            "Members with none of the configured roster roles",
            &model.members_with_none_of_configured_roster_roles,
        );
    }

    let counts = result.report.counts();
    out.push_str("\nNote:\n");
    out.push_str("  This is a read-only Discord diagnostic snapshot. It does not edit messages, modify roles, register commands, query Google Sheets, or write legacy JSON.\n");
    out.push_str("  Main/admin roster grouping uses current Discord role membership and legacy JSON/cache only.\n");
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_discord_snapshot_json(
    result: &ClanlistDiscordSnapshotResult,
) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();

    let envelope = ClanlistDiscordSnapshotOutput {
        mode: DISCORD_SNAPSHOT_MODE,
        safety: result.safety.clone(),
        guild_id: result.model.as_ref().map(|model| model.guild_id),
        targets: result.model.as_ref().map(|model| model.targets.clone()),
        role_count: result.model.as_ref().map(|model| model.role_count),
        member_count: result.model.as_ref().and_then(|model| model.member_count),
        configured_main_role_ids: result
            .model
            .as_ref()
            .map(|model| model.configured_main_role_ids.clone())
            .unwrap_or_default(),
        configured_admin_role_ids: result
            .model
            .as_ref()
            .map(|model| model.configured_admin_role_ids.clone())
            .unwrap_or_default(),
        missing_configured_roles: result
            .model
            .as_ref()
            .map(|model| model.missing_configured_roles.clone())
            .unwrap_or_default(),
        main_roles: result
            .model
            .as_ref()
            .map(|model| model.main_roles.clone())
            .unwrap_or_default(),
        admin_roles: result
            .model
            .as_ref()
            .map(|model| model.admin_roles.clone())
            .unwrap_or_default(),
        members_with_multiple_configured_main_roles: result
            .model
            .as_ref()
            .map(|model| model.members_with_multiple_configured_main_roles.clone())
            .unwrap_or_default(),
        members_with_multiple_configured_admin_roles: result
            .model
            .as_ref()
            .map(|model| model.members_with_multiple_configured_admin_roles.clone())
            .unwrap_or_default(),
        members_with_multiple_configured_roster_roles: result
            .model
            .as_ref()
            .map(|model| model.members_with_multiple_configured_roster_roles.clone())
            .unwrap_or_default(),
        members_with_none_of_configured_roster_roles: result
            .model
            .as_ref()
            .map(|model| model.members_with_none_of_configured_roster_roles.clone())
            .unwrap_or_default(),
        warnings,
        failures,
        note: DISCORD_SNAPSHOT_NOTE,
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn render_render_preview_text(result: &ClanlistRenderPreviewResult) -> String {
    render_render_preview_text_with_options(result, RenderTextOptions::default())
}

pub fn render_render_preview_text_with_options(
    result: &ClanlistRenderPreviewResult,
    options: RenderTextOptions,
) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Render Parity Preview\n");
    out.push_str("Mode: READ ONLY\n");
    out.push_str("Discord HTTP reads: ENABLED ONLY WITH --allow-discord-read\n");
    out.push_str("Discord Gateway: DISABLED\n");
    out.push_str("Discord writes: DISABLED\n");
    out.push_str("Message edits: DISABLED\n");
    out.push_str("Role modifications: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Migrations: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nDiscord read-only input:\n");
        out.push_str(&format!("  guild_id: {}\n", model.guild_id));
        out.push_str(&format!("  roles_fetched: {}\n", model.role_count));
        match model.member_count {
            Some(count) => out.push_str(&format!("  members_fetched: {count}\n")),
            None => out.push_str("  members_fetched: skipped by --roles-only\n"),
        }

        render_roster_panel_text(
            &mut out,
            "\nMain roster preview:",
            &model.main_panel,
            options.max_members_per_section,
        );
        render_roster_panel_text(
            &mut out,
            "\nAdmin roster preview:",
            &model.admin_panel,
            options.max_members_per_section,
        );
        if let Some(steam_panel) = &model.steam_panel {
            render_steam_panel_text(
                &mut out,
                "\nSteam roster preview:",
                steam_panel,
                options.max_members_per_section,
            );
        } else {
            out.push_str("\nSteam roster preview: omitted\n");
        }

        out.push_str("\nLimitations:\n");
        for limitation in &model.limitations {
            out.push_str(&format!("  - {limitation}\n"));
        }
    }

    let counts = result.report.counts();
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_render_preview_json(
    result: &ClanlistRenderPreviewResult,
) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();

    let envelope = ClanlistRenderPreviewOutput {
        mode: RENDER_PREVIEW_MODE,
        safety: result.safety.clone(),
        guild_id: result.model.as_ref().map(|model| model.guild_id),
        role_count: result.model.as_ref().map(|model| model.role_count),
        member_count: result.model.as_ref().and_then(|model| model.member_count),
        main_panel: result.model.as_ref().map(|model| model.main_panel.clone()),
        admin_panel: result.model.as_ref().map(|model| model.admin_panel.clone()),
        steam_panel: result
            .model
            .as_ref()
            .and_then(|model| model.steam_panel.clone()),
        warnings,
        failures,
        limitations: result
            .model
            .as_ref()
            .map(|model| model.limitations.clone())
            .unwrap_or_default(),
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn render_write_plan_text(result: &ClanlistWritePlanResult) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Write Plan\n");
    out.push_str("Mode: DRY RUN / NO WRITES\n");
    out.push_str("Discord reads: ENABLED ONLY WITH --allow-discord-read\n");
    out.push_str("Discord writes: DISABLED\n");
    out.push_str("Message edits: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Migrations: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nRender summary:\n");
        out.push_str(&format!("  guild_id: {}\n", model.render_summary.guild_id));
        out.push_str(&format!(
            "  roles_fetched: {}\n",
            model.render_summary.role_count
        ));
        match model.render_summary.member_count {
            Some(count) => out.push_str(&format!("  members_fetched: {count}\n")),
            None => out.push_str("  members_fetched: <unknown>\n"),
        }
        out.push_str(&format!(
            "  main_total_members: {}\n",
            optional_usize(model.render_summary.main_total_members)
        ));
        out.push_str(&format!(
            "  admin_total_members: {}\n",
            optional_usize(model.render_summary.admin_total_members)
        ));
        out.push_str(&format!(
            "  steam_records: active={} excluded={} unknown={}\n",
            optional_usize(model.render_summary.steam_active_records),
            optional_usize(model.render_summary.steam_excluded_records),
            optional_usize(model.render_summary.steam_unknown_member_records)
        ));

        out.push_str("\nPlanned operations:\n");
        for (index, operation) in model.planned_operations.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {} allowed={}\n",
                index + 1,
                operation.operation_type,
                operation.allowed
            ));
            out.push_str(&format!("     panel: {}\n", operation.panel_name));
            out.push_str(&format!("     channel_id: {}\n", operation.channel_id));
            out.push_str(&format!("     message_id: {}\n", operation.message_id));
            out.push_str(&format!(
                "     expected_title: {}\n",
                operation.expected_title
            ));
            out.push_str(&format!(
                "     expected_embed_count: {}\n",
                operation.expected_embed_count
            ));
            out.push_str(&format!(
                "     expected_message_chunk_count: {}\n",
                operation.expected_message_chunk_count
            ));
            out.push_str(&format!(
                "     {}: {}\n",
                operation.expected_total_label,
                optional_usize(operation.expected_total)
            ));
            out.push_str(&format!("     reason: {}\n", operation.reason));
            out.push_str("     rollback:\n");
            for note in &operation.rollback_notes {
                out.push_str(&format!("       - {note}\n"));
            }
        }

        out.push_str("\nLimitations:\n");
        for limitation in &model.limitations {
            out.push_str(&format!("  - {limitation}\n"));
        }
    }

    let counts = result.report.counts();
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_write_plan_json(
    result: &ClanlistWritePlanResult,
) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();

    let envelope = ClanlistWritePlanOutput {
        mode: WRITE_PLAN_MODE,
        safety: result.safety.clone(),
        render_summary: result
            .model
            .as_ref()
            .map(|model| model.render_summary.clone()),
        planned_operations: result
            .model
            .as_ref()
            .map(|model| model.planned_operations.clone())
            .unwrap_or_default(),
        checks: result
            .model
            .as_ref()
            .map(|model| model.checks.clone())
            .unwrap_or_default(),
        warnings,
        failures,
        rollback_notes: result
            .model
            .as_ref()
            .map(|model| model.rollback_notes.clone())
            .unwrap_or_default(),
        limitations: result
            .model
            .as_ref()
            .map(|model| model.limitations.clone())
            .unwrap_or_default(),
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn render_target_message_check_text(result: &ClanlistTargetMessageCheckResult) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Target Message Check\n");
    out.push_str("Mode: READ ONLY\n");
    out.push_str("Discord reads: ENABLED ONLY WITH --allow-discord-read\n");
    out.push_str("Discord writes: DISABLED\n");
    out.push_str("Message edits: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Migrations: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nCurrent bot:\n");
        out.push_str(&format!("  user_id: {}\n", model.current_bot_user_id));
        out.push_str(&format!(
            "  all_targets_editable_candidates: {}\n",
            model.all_targets_editable_candidates
        ));

        out.push_str("\nTarget messages:\n");
        for check in &model.target_checks {
            out.push_str(&format!(
                "  - panel: {} status={}\n",
                check.panel_name, check.status
            ));
            out.push_str(&format!("    channel_id: {}\n", check.channel_id));
            out.push_str(&format!("    message_id: {}\n", check.message_id));
            out.push_str(&format!("    message_exists: {}\n", check.message_exists));
            out.push_str(&format!(
                "    message_author_id: {}\n",
                optional_u64(check.message_author_id)
            ));
            out.push_str(&format!(
                "    current_bot_id: {}\n",
                check.current_bot_user_id
            ));
            out.push_str(&format!(
                "    editable_by_current_bot: {}\n",
                check.editable_by_current_bot
            ));
            out.push_str(&format!(
                "    embed_count: {}\n",
                optional_usize(check.embed_count)
            ));
            out.push_str(&format!("    expected_title: {}\n", check.expected_title));
            out.push_str(&format!(
                "    first_embed_title: {}\n",
                check
                    .actual_first_embed_title
                    .as_deref()
                    .unwrap_or("<missing>")
            ));
            out.push_str(&format!(
                "    title_roughly_matches: {}\n",
                check
                    .title_roughly_matches
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "<unknown>".to_owned())
            ));
            out.push_str(&format!(
                "    first_embed_footer_text: {}\n",
                check
                    .first_embed_footer_text
                    .as_deref()
                    .unwrap_or("<missing>")
            ));
            out.push_str(&format!(
                "    first_embed_footer_icon_url: {}\n",
                check
                    .first_embed_footer_icon_url
                    .as_deref()
                    .unwrap_or("<missing>")
            ));
            out.push_str(&format!(
                "    first_embed_marker_url: {}\n",
                check
                    .first_embed_marker_url
                    .as_deref()
                    .unwrap_or("<missing>")
            ));
            if let Some(reason) = &check.failure_reason {
                out.push_str(&format!("    failure_reason: {reason}\n"));
            }
        }

        out.push_str("\nLimitations:\n");
        for limitation in &model.limitations {
            out.push_str(&format!("  - {limitation}\n"));
        }
    }

    let counts = result.report.counts();
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_target_message_check_json(
    result: &ClanlistTargetMessageCheckResult,
) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();

    let envelope = ClanlistTargetMessageCheckOutput {
        mode: TARGET_MESSAGE_CHECK_MODE,
        safety: result.safety.clone(),
        current_bot_user_id: result.model.as_ref().map(|model| model.current_bot_user_id),
        target_checks: result
            .model
            .as_ref()
            .map(|model| model.target_checks.clone())
            .unwrap_or_default(),
        all_targets_editable_candidates: result
            .model
            .as_ref()
            .map(|model| model.all_targets_editable_candidates),
        warnings,
        failures,
        limitations: result
            .model
            .as_ref()
            .map(|model| model.limitations.clone())
            .unwrap_or_default(),
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn render_bootstrap_new_panels_text(result: &ClanlistBootstrapNewPanelsResult) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Fresh Panel Bootstrap\n");
    out.push_str(if result.safety.dry_run {
        "Mode: DRY RUN / NO WRITES\n"
    } else {
        "Mode: WRITE-CAPABLE / CREATE NEW MESSAGES ONLY\n"
    });
    out.push_str("Discord Gateway: DISABLED\n");
    out.push_str("Discord HTTP: ENABLED ONLY WITH EXPLICIT FLAGS\n");
    out.push_str(&format!(
        "Discord message creates: {}\n",
        if result.safety.discord_message_creates {
            "ENABLED FOR EXACTLY 3 NEW MESSAGES"
        } else {
            "DISABLED"
        }
    ));
    out.push_str("Discord message edits: DISABLED\n");
    out.push_str("Discord message deletes: DISABLED\n");
    out.push_str("Role modifications: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Legacy DB writes: DISABLED\n");
    out.push_str("Allowed mentions: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nRender summary:\n");
        out.push_str(&format!("  guild_id: {}\n", model.guild_id));
        out.push_str(&format!("  bot_user_id: {}\n", model.bot_user_id));
        out.push_str(&format!(
            "  roles_fetched: {}\n",
            model.render_summary.role_count
        ));
        out.push_str(&format!(
            "  members_fetched: {}\n",
            optional_usize(model.render_summary.member_count)
        ));
        out.push_str(&format!(
            "  main_total_members: {}\n",
            optional_usize(model.render_summary.main_total_members)
        ));
        out.push_str(&format!(
            "  admin_total_members: {}\n",
            optional_usize(model.render_summary.admin_total_members)
        ));
        out.push_str(&format!(
            "  steam_records: active={} excluded={} unknown={}\n",
            optional_usize(model.render_summary.steam_active_records),
            optional_usize(model.render_summary.steam_excluded_records),
            optional_usize(model.render_summary.steam_unknown_member_records)
        ));

        out.push_str("\nOld legacy message IDs, reference only:\n");
        out.push_str(&format!("  main: {}\n", model.old_legacy_message_ids.main));
        out.push_str(&format!(
            "  admin: {}\n",
            model.old_legacy_message_ids.admin
        ));
        out.push_str(&format!(
            "  steam: {}\n",
            model.old_legacy_message_ids.steam
        ));

        out.push_str("\nCreate operations:\n");
        for operation in &model.operations {
            out.push_str(&format!(
                "  - {} panel: {} allowed={}\n",
                operation.panel_name, operation.operation_type, operation.allowed
            ));
            out.push_str(&format!("    status: {}\n", operation.status));
            out.push_str(&format!("    channel_id: {}\n", operation.channel_id));
            out.push_str(&format!(
                "    old_legacy_message_id: {}\n",
                operation.legacy_message_id
            ));
            out.push_str(&format!(
                "    new_message_id: {}\n",
                optional_u64(operation.new_message_id)
            ));
            out.push_str(&format!(
                "    expected_title: {}\n",
                operation.expected_title
            ));
            out.push_str(&format!(
                "    expected_embed_count: {}\n",
                operation.expected_embed_count
            ));
            if let Some(reason) = &operation.failure_reason {
                out.push_str(&format!("    failure_reason: {reason}\n"));
            }
        }

        if let Some(path) = &model.state_file_path {
            out.push_str(&format!("\nState file written: {path}\n"));
        }
        if let Some(path) = &model.partial_recovery_file_path {
            out.push_str(&format!("\nPartial recovery file written: {path}\n"));
        }

        out.push_str("\nManual next steps:\n");
        for step in &model.manual_next_steps {
            out.push_str(&format!("  - {step}\n"));
        }

        out.push_str("\nLimitations:\n");
        for limitation in &model.limitations {
            out.push_str(&format!("  - {limitation}\n"));
        }
    }

    let counts = result.report.counts();
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_bootstrap_new_panels_json(
    result: &ClanlistBootstrapNewPanelsResult,
) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();

    let envelope = ClanlistBootstrapNewPanelsOutput {
        mode: BOOTSTRAP_NEW_PANELS_MODE,
        safety: result.safety.clone(),
        dry_run: result.safety.dry_run,
        guild_id: result.model.as_ref().map(|model| model.guild_id),
        bot_user_id: result.model.as_ref().map(|model| model.bot_user_id),
        render_summary: result
            .model
            .as_ref()
            .map(|model| model.render_summary.clone()),
        old_legacy_message_ids: result
            .model
            .as_ref()
            .map(|model| model.old_legacy_message_ids.clone()),
        payloads: result
            .model
            .as_ref()
            .map(|model| model.payloads.clone())
            .unwrap_or_default(),
        operations: result
            .model
            .as_ref()
            .map(|model| model.operations.clone())
            .unwrap_or_default(),
        state_file_path: result
            .model
            .as_ref()
            .and_then(|model| model.state_file_path.clone()),
        partial_recovery_file_path: result
            .model
            .as_ref()
            .and_then(|model| model.partial_recovery_file_path.clone()),
        warnings,
        failures,
        manual_next_steps: result
            .model
            .as_ref()
            .map(|model| model.manual_next_steps.clone())
            .unwrap_or_default(),
        limitations: result
            .model
            .as_ref()
            .map(|model| model.limitations.clone())
            .unwrap_or_default(),
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn render_update_panels_text(result: &ClanlistUpdatePanelsResult) -> String {
    let mut out = String::new();
    out.push_str("XIII Clanlist Update Panels\n");
    out.push_str(if result.safety.dry_run {
        "Mode: DRY RUN / NO WRITES\n"
    } else {
        "Mode: WRITE-CAPABLE / EDIT EXISTING NEW PANELS ONLY\n"
    });
    out.push_str("Discord Gateway: DISABLED\n");
    out.push_str("Discord HTTP: ENABLED ONLY WITH EXPLICIT FLAGS\n");
    out.push_str("Discord message creates: DISABLED\n");
    out.push_str(&format!(
        "Discord message edits: {}\n",
        if result.safety.discord_message_edits {
            "ENABLED FOR EXACTLY 3 STATE TARGETS"
        } else {
            "DISABLED"
        }
    ));
    out.push_str("Discord message deletes: DISABLED\n");
    out.push_str("Role modifications: DISABLED\n");
    out.push_str("Google Sheets: DISABLED\n");
    out.push_str("Legacy JSON writes: DISABLED\n");
    out.push_str("Legacy DB writes: DISABLED\n");
    out.push_str("Allowed mentions: DISABLED\n\n");

    for item in &result.report.items {
        out.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }

    if let Some(model) = &result.model {
        out.push_str("\nState:\n");
        out.push_str(&format!("  state_file: {}\n", model.state_file_path));
        out.push_str(&format!("  guild_id: {}\n", model.guild_id));
        out.push_str(&format!("  bot_user_id: {}\n", model.bot_user_id));

        out.push_str("\nRender summary:\n");
        out.push_str(&format!(
            "  roles_fetched: {}\n",
            model.render_summary.role_count
        ));
        out.push_str(&format!(
            "  members_fetched: {}\n",
            optional_usize(model.render_summary.member_count)
        ));
        out.push_str(&format!(
            "  main_total_members: {}\n",
            optional_usize(model.render_summary.main_total_members)
        ));
        out.push_str(&format!(
            "  admin_total_members: {}\n",
            optional_usize(model.render_summary.admin_total_members)
        ));
        out.push_str(&format!(
            "  steam_records: active={} excluded={} unknown={}\n",
            optional_usize(model.render_summary.steam_active_records),
            optional_usize(model.render_summary.steam_excluded_records),
            optional_usize(model.render_summary.steam_unknown_member_records)
        ));

        out.push_str("\nTarget checks:\n");
        for check in &model.target_checks {
            out.push_str(&format!(
                "  - panel={} message_id={} exists={} editable_by_current_bot={} status={}\n",
                check.panel_name,
                check.message_id,
                check.message_exists,
                check.editable_by_current_bot,
                check.status
            ));
            if let Some(reason) = &check.failure_reason {
                out.push_str(&format!("    failure_reason: {reason}\n"));
            }
        }

        out.push_str("\nPlanned/actual operations:\n");
        for (index, operation) in model.operations.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {} allowed={} panel={} message_id={}\n",
                index + 1,
                operation.operation_type,
                operation.allowed,
                operation.panel_name,
                operation.message_id
            ));
            out.push_str(&format!("     status: {}\n", operation.status));
            out.push_str(&format!("     channel_id: {}\n", operation.channel_id));
            out.push_str(&format!(
                "     edited_message_id: {}\n",
                optional_u64(operation.edited_message_id)
            ));
            out.push_str(&format!(
                "     expected_title: {}\n",
                operation.expected_title
            ));
            out.push_str(&format!(
                "     expected_embed_count: {}\n",
                operation.expected_embed_count
            ));
            if let Some(reason) = &operation.failure_reason {
                out.push_str(&format!("     failure_reason: {reason}\n"));
            }
        }

        if let Some(path) = &model.state_updated_path {
            out.push_str(&format!("\nState updated: {path}\n"));
        }
        if let Some(path) = &model.partial_recovery_file_path {
            out.push_str(&format!("\nPartial update file written: {path}\n"));
        }

        out.push_str("\nManual next steps:\n");
        for step in &model.manual_next_steps {
            out.push_str(&format!("  - {step}\n"));
        }

        out.push_str("\nLimitations:\n");
        for limitation in &model.limitations {
            out.push_str(&format!("  - {limitation}\n"));
        }
    }

    let counts = result.report.counts();
    out.push_str(&format!(
        "\nSummary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    out
}

pub fn render_update_panels_json(
    result: &ClanlistUpdatePanelsResult,
) -> Result<String, serde_json::Error> {
    let warnings = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Warn)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();
    let failures = result
        .report
        .items
        .iter()
        .filter(|item| item.severity == xiii_core::Severity::Fail)
        .map(|item| format!("{} {}", item.scope, item.message))
        .collect();

    let envelope = ClanlistUpdatePanelsOutput {
        mode: UPDATE_PANELS_MODE,
        safety: result.safety.clone(),
        dry_run: result.safety.dry_run,
        state_file_path: result
            .model
            .as_ref()
            .map(|model| model.state_file_path.clone()),
        guild_id: result.model.as_ref().map(|model| model.guild_id),
        bot_user_id: result.model.as_ref().map(|model| model.bot_user_id),
        render_summary: result
            .model
            .as_ref()
            .map(|model| model.render_summary.clone()),
        target_checks: result
            .model
            .as_ref()
            .map(|model| model.target_checks.clone())
            .unwrap_or_default(),
        payloads: result
            .model
            .as_ref()
            .map(|model| model.payloads.clone())
            .unwrap_or_default(),
        operations: result
            .model
            .as_ref()
            .map(|model| model.operations.clone())
            .unwrap_or_default(),
        state_updated_path: result
            .model
            .as_ref()
            .and_then(|model| model.state_updated_path.clone()),
        partial_recovery_file_path: result
            .model
            .as_ref()
            .and_then(|model| model.partial_recovery_file_path.clone()),
        warnings,
        failures,
        manual_next_steps: result
            .model
            .as_ref()
            .map(|model| model.manual_next_steps.clone())
            .unwrap_or_default(),
        limitations: result
            .model
            .as_ref()
            .map(|model| model.limitations.clone())
            .unwrap_or_default(),
    };
    serde_json::to_string_pretty(&envelope)
}

pub fn parse_message_id_json(text: &str) -> Result<u64, String> {
    let value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    collect_snowflakes(&value)
        .into_iter()
        .next()
        .ok_or_else(|| "no Discord snowflake message ID found".to_owned())
}

pub fn parse_steam_cache_count(text: &str) -> Result<usize, String> {
    parse_steam_cache(text).map(|records| records.len())
}

pub fn parse_steam_cache(text: &str) -> Result<Vec<SteamCacheRecord>, String> {
    let value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let records_value = value.get("records").unwrap_or(&value);
    match records_value {
        Value::Object(map) => {
            let mut records = Vec::with_capacity(map.len());
            for (key, value) in map {
                if let Value::Object(_) = value {
                    let mut record = parse_steam_record(value)?;
                    if record.discord_id.is_empty() {
                        record.discord_id = key.clone();
                    }
                    records.push(record);
                }
            }
            records.sort_by(|left, right| left.discord_id.cmp(&right.discord_id));
            Ok(records)
        }
        Value::Array(items) => {
            let mut records = Vec::with_capacity(items.len());
            for value in items {
                records.push(parse_steam_record(value)?);
            }
            Ok(records)
        }
        _ => Err("steam cache must be an object or array".to_owned()),
    }
}

fn roster_write_operation(panel: &RenderedRosterPanel) -> PlannedWriteOperation {
    let expected_embed_count = 1 + panel
        .role_sections
        .iter()
        .map(|section| section.embed_chunks.len())
        .sum::<usize>();
    PlannedWriteOperation {
        operation_type: "edit_existing_message",
        allowed: false,
        reason: "dry-run write plan only",
        panel_name: panel.panel_name,
        channel_id: panel.target.channel_id,
        message_id: panel.target.message_id,
        expected_title: panel.header.title.to_owned(),
        expected_embed_count,
        expected_message_chunk_count: message_chunk_count(expected_embed_count),
        expected_total_label: "expected_total_members".to_owned(),
        expected_total: panel.total_members,
        safety_checks: common_operation_safety_checks(),
        rollback_notes: common_operation_rollback_notes(),
    }
}

fn steam_write_operation(panel: Option<&RenderedSteamPanel>) -> PlannedWriteOperation {
    match panel {
        Some(panel) => {
            let expected_embed_count =
                1 + panel.active_block.embed_chunks.len() + panel.excluded_block.embed_chunks.len();
            PlannedWriteOperation {
                operation_type: "edit_existing_message",
                allowed: false,
                reason: "dry-run write plan only",
                panel_name: "steam",
                channel_id: panel.target.channel_id,
                message_id: panel.target.message_id,
                expected_title: panel.header.title.to_owned(),
                expected_embed_count,
                expected_message_chunk_count: message_chunk_count(expected_embed_count),
                expected_total_label: "expected_total_records".to_owned(),
                expected_total: Some(
                    panel.active_block.entries.len() + panel.excluded_block.entries.len(),
                ),
                safety_checks: common_operation_safety_checks(),
                rollback_notes: common_operation_rollback_notes(),
            }
        }
        None => PlannedWriteOperation {
            operation_type: "edit_existing_message",
            allowed: false,
            reason: "dry-run write plan only",
            panel_name: "steam",
            channel_id: 0,
            message_id: 0,
            expected_title: STEAM_PANEL_TITLE.to_owned(),
            expected_embed_count: 0,
            expected_message_chunk_count: 0,
            expected_total_label: "expected_total_records".to_owned(),
            expected_total: None,
            safety_checks: common_operation_safety_checks(),
            rollback_notes: common_operation_rollback_notes(),
        },
    }
}

fn message_chunk_count(embed_count: usize) -> usize {
    if embed_count == 0 {
        0
    } else {
        embed_count.div_ceil(MAX_EMBEDS_PER_MESSAGE)
    }
}

fn common_operation_safety_checks() -> Vec<String> {
    vec![
        "allowed=false".to_owned(),
        "Discord writes disabled".to_owned(),
        "legacy JSON writes disabled".to_owned(),
        "Google Sheets disabled".to_owned(),
        "write_state_allowed=false".to_owned(),
    ]
}

fn common_operation_rollback_notes() -> Vec<String> {
    vec![
        "No rollback is needed for this command because it performs no writes.".to_owned(),
        "For a future writer cutover, rollback is to stop the superbot writer and restart the old xiii-clanlist.service.".to_owned(),
    ]
}

fn add_plan_check(
    report: &mut Report,
    checks: &mut Vec<WritePlanCheck>,
    severity: xiii_core::Severity,
    name: impl Into<String>,
    detail: impl Into<String>,
) {
    let name = name.into();
    let detail = detail.into();
    report.push(severity, "write_plan", format!("{name}: {detail}"));
    checks.push(WritePlanCheck {
        status: severity.as_str().to_owned(),
        name,
        detail,
    });
}

fn optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<unknown>".to_owned())
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<unknown>".to_owned())
}

fn build_roster_render_panel(
    panel_name: &'static str,
    panel_title: &'static str,
    marker_url: &'static str,
    target: RenderTarget,
    role_order: &[u64],
    role_map: &BTreeMap<u64, String>,
    members: Option<&[DiscordMemberSnapshotInput]>,
    report: &mut Report,
) -> RenderedRosterPanel {
    let mut assigned_member_ids = BTreeSet::new();
    let mut role_sections = Vec::new();
    let mut omitted_role_ids = Vec::new();
    let mut total_members = 0usize;

    for role_id in role_order {
        let Some(role_name) = role_map.get(role_id) else {
            report.warn(
                "clanlist",
                format!("{panel_name} configured role missing in guild: {role_id}"),
            );
            omitted_role_ids.push(*role_id);
            continue;
        };

        let Some(members) = members else {
            role_sections.push(RenderedRoleSection {
                role_id: *role_id,
                role_name: role_name.clone(),
                member_count: 0,
                members: Vec::new(),
                embed_chunks: Vec::new(),
            });
            continue;
        };

        let mut role_members = members
            .iter()
            .filter(|member| {
                member.role_ids.contains(role_id) && !assigned_member_ids.contains(&member.user_id)
            })
            .map(render_member_source)
            .collect::<Vec<_>>();
        sort_render_member_sources(&mut role_members);

        if role_members.is_empty() {
            report.warn(
                "clanlist",
                format!(
                    "{panel_name} role has zero displayable members: role_id={} role_name={}",
                    role_id, role_name
                ),
            );
            omitted_role_ids.push(*role_id);
            continue;
        }

        let rendered_members = role_members
            .iter()
            .enumerate()
            .map(|(index, member)| RenderedMemberLine {
                position: index + 1,
                user_id: member.user_id,
                display_name: member.display_name.clone(),
                rendered_line: format!("{}. <@{}>", index + 1, member.user_id),
            })
            .collect::<Vec<_>>();
        let rendered_lines = rendered_members
            .iter()
            .map(|member| member.rendered_line.clone())
            .collect::<Vec<_>>();
        let embed_chunks = role_embed_chunks(role_name, rendered_members.len(), &rendered_lines);

        for member in &role_members {
            assigned_member_ids.insert(member.user_id);
        }
        total_members += role_members.len();
        role_sections.push(RenderedRoleSection {
            role_id: *role_id,
            role_name: role_name.clone(),
            member_count: rendered_members.len(),
            members: rendered_members,
            embed_chunks,
        });
    }

    RenderedRosterPanel {
        panel_name,
        target,
        header: RenderedHeaderPreview {
            title: panel_title,
            description: match members {
                Some(_) => format!("**{}: {}**", RU_TOTAL_MEMBERS, total_members),
                None => format!("**{}: <members skipped>**", RU_TOTAL_MEMBERS),
            },
            footer_template: RU_UPDATED_FOOTER_TEMPLATE,
            marker_url,
            color_hex: EMBED_COLOR_HEX,
        },
        total_members: members.map(|_| total_members),
        role_sections,
        omitted_role_ids,
    }
}

fn build_steam_render_panel(
    target: RenderTarget,
    active_role_id: u64,
    records: &[SteamCacheRecord],
    members: Option<&[DiscordMemberSnapshotInput]>,
    report: &mut Report,
) -> RenderedSteamPanel {
    let member_map = members.map(|members| {
        members
            .iter()
            .map(|member| (member.user_id, member))
            .collect::<BTreeMap<_, _>>()
    });
    let mut active_sources = Vec::new();
    let mut excluded_sources = Vec::new();
    let mut unknown_sources = Vec::new();

    for record in records {
        let Ok(user_id) = record.discord_id.parse::<u64>() else {
            report.warn(
                "clanlist",
                format!(
                    "Steam cache record has invalid Discord ID and is omitted: {}",
                    record.discord_id
                ),
            );
            continue;
        };

        let member = member_map
            .as_ref()
            .and_then(|member_map| member_map.get(&user_id).copied());
        let display_name = member
            .map(|member| member.display_name.clone())
            .or_else(|| record.last_display_name.clone());

        let status = match (members, member) {
            (None, _) => "member_fetch_skipped",
            (Some(_), Some(member)) if member.role_ids.contains(&active_role_id) => "active",
            (Some(_), Some(_)) => "excluded",
            (Some(_), None) => "unknown_member",
        };

        let source = SteamEntrySource {
            discord_id: record.discord_id.clone(),
            user_id: Some(user_id),
            display_name,
            steam_id64: record.steam_id64.clone(),
            status: status.to_owned(),
        };

        match status {
            "active" => active_sources.push(source),
            "unknown_member" => {
                report.warn(
                    "clanlist",
                    format!(
                        "cached Steam Discord ID not found in fetched members: {}",
                        record.discord_id
                    ),
                );
                unknown_sources.push(source.clone());
                excluded_sources.push(source);
            }
            "member_fetch_skipped" => {
                report.warn(
                    "clanlist",
                    "Steam active/excluded split cannot be refreshed because members were skipped by --roles-only",
                );
                excluded_sources.push(source);
            }
            _ => excluded_sources.push(source),
        }
    }

    sort_steam_sources(&mut active_sources);
    sort_steam_sources(&mut excluded_sources);
    sort_steam_sources(&mut unknown_sources);

    let active_entries = render_steam_entries(&active_sources);
    let excluded_entries = render_steam_entries(&excluded_sources);
    let unknown_entries = render_steam_entries(&unknown_sources);

    let active_count = members.map(|_| active_entries.len());
    let excluded_count = members.map(|_| excluded_entries.len());
    let active_description = active_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "<members skipped>".to_owned());
    let excluded_description = excluded_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "<members skipped>".to_owned());

    RenderedSteamPanel {
        panel_name: "steam",
        target,
        source: "legacy steam cache",
        header: RenderedHeaderPreview {
            title: STEAM_PANEL_TITLE,
            description: format!(
                "**{}: {}**\n**{}: {}**",
                RU_ACTIVE_MEMBERS, active_description, RU_EXCLUDED_MEMBERS, excluded_description
            ),
            footer_template: RU_UPDATED_FOOTER_TEMPLATE,
            marker_url: STEAM_PANEL_MARKER_URL,
            color_hex: EMBED_COLOR_HEX,
        },
        active_count,
        excluded_count,
        unknown_member_count: unknown_entries.len(),
        active_block: steam_block(RU_ACTIVE_MEMBERS, active_entries),
        excluded_block: steam_block(RU_EXCLUDED_MEMBERS, excluded_entries),
        unknown_member_entries: unknown_entries,
    }
}

#[derive(Debug, Clone)]
struct RenderMemberSource {
    user_id: u64,
    display_name: String,
}

#[derive(Debug, Clone)]
struct SteamEntrySource {
    discord_id: String,
    user_id: Option<u64>,
    display_name: Option<String>,
    steam_id64: String,
    status: String,
}

fn render_member_source(member: &DiscordMemberSnapshotInput) -> RenderMemberSource {
    RenderMemberSource {
        user_id: member.user_id,
        display_name: member.display_name.clone(),
    }
}

fn sort_render_member_sources(members: &mut [RenderMemberSource]) {
    members.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
}

fn sort_steam_sources(entries: &mut [SteamEntrySource]) {
    entries.sort_by(|left, right| {
        steam_sort_key(left)
            .cmp(&steam_sort_key(right))
            .then_with(|| left.discord_id.cmp(&right.discord_id))
    });
}

fn steam_sort_key(entry: &SteamEntrySource) -> (String, u64) {
    let display_name = entry
        .display_name
        .as_deref()
        .unwrap_or(&entry.discord_id)
        .to_lowercase();
    let discord_id = entry.discord_id.parse::<u64>().unwrap_or(u64::MAX);
    (display_name, discord_id)
}

fn role_embed_chunks(
    role_name: &str,
    member_count: usize,
    rendered_lines: &[String],
) -> Vec<RenderedEmbedChunk> {
    chunk_lines(rendered_lines, MAX_FIELD_VALUE_LENGTH)
        .into_iter()
        .enumerate()
        .map(|(index, lines)| {
            let title = if index == 0 {
                format!("{role_name} ({member_count})")
            } else {
                format!("{role_name} ({RU_CONTINUATION})")
            };
            let description = lines.join("\n");
            RenderedEmbedChunk {
                title,
                description,
                line_count: lines.len(),
            }
        })
        .collect()
}

fn render_steam_entries(entries: &[SteamEntrySource]) -> Vec<RenderedSteamEntry> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| RenderedSteamEntry {
            position: index + 1,
            discord_id: entry.discord_id.clone(),
            user_id: entry.user_id,
            display_name: entry.display_name.clone(),
            steam_id64: entry.steam_id64.clone(),
            status: entry.status.clone(),
            rendered_entry: format!(
                "{}. <@{}>\n   Steam ID64: `{}`",
                index + 1,
                entry.discord_id,
                entry.steam_id64
            ),
        })
        .collect()
}

fn steam_block(title_prefix: &str, entries: Vec<RenderedSteamEntry>) -> RenderedSteamBlock {
    let title = format!("{title_prefix} ({})", entries.len());
    if entries.is_empty() {
        return RenderedSteamBlock {
            title: title.clone(),
            entries,
            embed_chunks: vec![RenderedEmbedChunk {
                title,
                description: RU_NO_RECORDS.to_owned(),
                line_count: 1,
            }],
        };
    }

    let rendered_lines = entries
        .iter()
        .map(|entry| entry.rendered_entry.clone())
        .collect::<Vec<_>>();
    let embed_chunks = chunk_lines(&rendered_lines, STEAM_BLOCK_VALUE_LENGTH)
        .into_iter()
        .enumerate()
        .map(|(index, lines)| {
            let chunk_title = if index == 0 {
                title.clone()
            } else {
                format!("{title_prefix} ({RU_CONTINUATION})")
            };
            RenderedEmbedChunk {
                title: chunk_title,
                description: lines.join("\n"),
                line_count: lines.len(),
            }
        })
        .collect();

    RenderedSteamBlock {
        title,
        entries,
        embed_chunks,
    }
}

fn chunk_lines(lines: &[String], limit: usize) -> Vec<Vec<String>> {
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut current_length = 0usize;

    for line in lines {
        let extra_length = if current_lines.is_empty() {
            line.len()
        } else {
            line.len() + 1
        };
        if !current_lines.is_empty() && current_length + extra_length > limit {
            chunks.push(current_lines);
            current_lines = vec![line.clone()];
            current_length = line.len();
            continue;
        }

        current_lines.push(line.clone());
        current_length += extra_length;
    }

    if !current_lines.is_empty() {
        chunks.push(current_lines);
    }
    chunks
}

fn warn_multiple_panel_roles(
    panel_name: &str,
    members: &[DiscordMemberSnapshotInput],
    configured_role_ids: &[u64],
    report: &mut Report,
) {
    for member in multiple_role_matches(members, configured_role_ids) {
        report.warn(
            "clanlist",
            format!(
                "{panel_name} member is in multiple configured rank roles and will render only in the earliest configured role: user_id={} role_ids={}",
                member.user_id,
                join_ids(&member.matching_role_ids)
            ),
        );
    }
}

fn build_roster_roles(
    role_order: &[u64],
    role_map: &BTreeMap<u64, String>,
    members: Option<&[DiscordMemberSnapshotInput]>,
) -> Vec<SnapshotRosterRole> {
    role_order
        .iter()
        .map(|role_id| {
            let mut role_members = members
                .map(|members| {
                    members
                        .iter()
                        .filter(|member| member.role_ids.contains(role_id))
                        .map(member_summary)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            sort_members(&mut role_members);
            SnapshotRosterRole {
                role_id: *role_id,
                role_name: role_map.get(role_id).cloned(),
                member_count: members.map(|_| role_members.len()),
                members: role_members,
            }
        })
        .collect()
}

fn configured_role_set(main_role_ids: &[u64], admin_role_ids: &[u64]) -> BTreeSet<u64> {
    main_role_ids
        .iter()
        .chain(admin_role_ids.iter())
        .copied()
        .collect()
}

fn multiple_role_matches(
    members: &[DiscordMemberSnapshotInput],
    configured_role_ids: &[u64],
) -> Vec<SnapshotMemberRoleMatch> {
    let configured: BTreeSet<u64> = configured_role_ids.iter().copied().collect();
    let mut matches = members
        .iter()
        .filter_map(|member| {
            let matching_role_ids = matching_role_ids(member, &configured);
            (matching_role_ids.len() > 1).then(|| SnapshotMemberRoleMatch {
                user_id: member.user_id,
                display_name: member.display_name.clone(),
                matching_role_ids,
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    matches
}

fn members_without_configured_roles(
    members: &[DiscordMemberSnapshotInput],
    configured_role_ids: &BTreeSet<u64>,
) -> Vec<SnapshotMemberSummary> {
    let mut unmatched = members
        .iter()
        .filter(|member| matching_role_ids(member, configured_role_ids).is_empty())
        .map(member_summary)
        .collect::<Vec<_>>();
    sort_members(&mut unmatched);
    unmatched
}

fn matching_role_ids(
    member: &DiscordMemberSnapshotInput,
    configured_role_ids: &BTreeSet<u64>,
) -> Vec<u64> {
    let mut matching = member
        .role_ids
        .iter()
        .copied()
        .filter(|role_id| configured_role_ids.contains(role_id))
        .collect::<Vec<_>>();
    matching.sort_unstable();
    matching.dedup();
    matching
}

fn member_summary(member: &DiscordMemberSnapshotInput) -> SnapshotMemberSummary {
    SnapshotMemberSummary {
        user_id: member.user_id,
        display_name: member.display_name.clone(),
    }
}

fn sort_members(members: &mut [SnapshotMemberSummary]) {
    members.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
}

fn render_role_rows(out: &mut String, roles: &[SnapshotRosterRole]) {
    for role in roles {
        let name = role.role_name.as_deref().unwrap_or("<missing>");
        let count = role
            .member_count
            .map(|count| count.to_string())
            .unwrap_or_else(|| "skipped".to_owned());
        out.push_str(&format!(
            "  {} | {} | members={}\n",
            role.role_id, name, count
        ));
        for member in role.members.iter().take(20) {
            out.push_str(&format!(
                "    - {} | {}\n",
                member.user_id, member.display_name
            ));
        }
        if role.members.len() > 20 {
            out.push_str(&format!(
                "    ... {} more members omitted from text output\n",
                role.members.len() - 20
            ));
        }
    }
}

fn render_roster_panel_text(
    out: &mut String,
    title: &str,
    panel: &RenderedRosterPanel,
    max_members_per_section: usize,
) {
    out.push_str(title);
    out.push('\n');
    out.push_str(&format!(
        "  target_channel_id: {}\n",
        panel.target.channel_id
    ));
    out.push_str(&format!(
        "  target_message_id: {}\n",
        panel.target.message_id
    ));
    out.push_str(&format!("  rendered_title: {}\n", panel.header.title));
    out.push_str(&format!(
        "  rendered_header_description: {}\n",
        panel.header.description
    ));
    out.push_str(&format!("  marker_url: {}\n", panel.header.marker_url));
    match panel.total_members {
        Some(total) => out.push_str(&format!("  total_members: {total}\n")),
        None => out.push_str("  total_members: skipped by --roles-only\n"),
    }
    if !panel.omitted_role_ids.is_empty() {
        out.push_str("  omitted_role_ids:\n");
        for role_id in &panel.omitted_role_ids {
            out.push_str(&format!("    - {role_id}\n"));
        }
    }
    out.push_str("  role_sections:\n");
    if panel.role_sections.is_empty() {
        out.push_str("    - <none>\n");
        return;
    }
    for section in &panel.role_sections {
        out.push_str(&format!(
            "    - {} | role_id={} | members={}\n",
            section.role_name, section.role_id, section.member_count
        ));
        if let Some(first_chunk) = section.embed_chunks.first() {
            out.push_str(&format!("      embed_title: {}\n", first_chunk.title));
        }
        for member in section.members.iter().take(max_members_per_section) {
            out.push_str(&format!(
                "      {} | display_name={}\n",
                member.rendered_line, member.display_name
            ));
        }
        if section.members.len() > max_members_per_section {
            out.push_str(&format!(
                "      ... {} more omitted\n",
                section.members.len() - max_members_per_section
            ));
        }
    }
}

fn render_steam_panel_text(
    out: &mut String,
    title: &str,
    panel: &RenderedSteamPanel,
    max_members_per_section: usize,
) {
    out.push_str(title);
    out.push('\n');
    out.push_str(&format!(
        "  target_channel_id: {}\n",
        panel.target.channel_id
    ));
    out.push_str(&format!(
        "  target_message_id: {}\n",
        panel.target.message_id
    ));
    out.push_str(&format!("  source: {}\n", panel.source));
    out.push_str(&format!("  rendered_title: {}\n", panel.header.title));
    out.push_str(&format!(
        "  rendered_header_description: {}\n",
        panel.header.description
    ));
    match panel.active_count {
        Some(count) => out.push_str(&format!("  active_count: {count}\n")),
        None => out.push_str("  active_count: skipped by --roles-only\n"),
    }
    match panel.excluded_count {
        Some(count) => out.push_str(&format!("  excluded_count: {count}\n")),
        None => out.push_str("  excluded_count: skipped by --roles-only\n"),
    }
    out.push_str(&format!(
        "  unknown_member_count: {}\n",
        panel.unknown_member_count
    ));
    if !panel.unknown_member_entries.is_empty() {
        out.push_str("  unknown_member_entries:\n");
        for entry in panel
            .unknown_member_entries
            .iter()
            .take(max_members_per_section)
        {
            out.push_str(&format!(
                "    - discord_id={} steam_id64={} display_name={}\n",
                entry.discord_id,
                entry.steam_id64,
                entry.display_name.as_deref().unwrap_or("<unknown>")
            ));
        }
        if panel.unknown_member_entries.len() > max_members_per_section {
            out.push_str(&format!(
                "    ... {} more omitted\n",
                panel.unknown_member_entries.len() - max_members_per_section
            ));
        }
    }
    render_steam_block_text(
        out,
        "  active_entries",
        &panel.active_block,
        max_members_per_section,
    );
    render_steam_block_text(
        out,
        "  excluded_entries",
        &panel.excluded_block,
        max_members_per_section,
    );
}

fn render_steam_block_text(
    out: &mut String,
    label: &str,
    block: &RenderedSteamBlock,
    max_members_per_section: usize,
) {
    out.push_str(&format!("{label}: {}\n", block.entries.len()));
    out.push_str(&format!("    embed_title: {}\n", block.title));
    for entry in block.entries.iter().take(max_members_per_section) {
        out.push_str(&format!(
            "    {} | status={} | display_name={}\n",
            entry.rendered_entry.replace('\n', " "),
            entry.status,
            entry.display_name.as_deref().unwrap_or("<unknown>")
        ));
    }
    if block.entries.len() > max_members_per_section {
        out.push_str(&format!(
            "    ... {} more omitted\n",
            block.entries.len() - max_members_per_section
        ));
    }
}

fn render_member_role_matches(out: &mut String, title: &str, members: &[SnapshotMemberRoleMatch]) {
    if members.is_empty() {
        return;
    }
    out.push_str(&format!("\n{title}:\n"));
    for member in members.iter().take(30) {
        out.push_str(&format!(
            "  - user_id={} display_name={} role_ids={}\n",
            member.user_id,
            member.display_name,
            join_ids(&member.matching_role_ids)
        ));
    }
    if members.len() > 30 {
        out.push_str(&format!(
            "  ... {} more members omitted from text output\n",
            members.len() - 30
        ));
    }
}

fn render_member_list(out: &mut String, title: &str, members: &[SnapshotMemberSummary]) {
    if members.is_empty() {
        return;
    }
    out.push_str(&format!("\n{title}: {}\n", members.len()));
    for member in members.iter().take(30) {
        out.push_str(&format!(
            "  - user_id={} display_name={}\n",
            member.user_id, member.display_name
        ));
    }
    if members.len() > 30 {
        out.push_str(&format!(
            "  ... {} more members omitted from text output\n",
            members.len() - 30
        ));
    }
}

fn join_ids(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

fn parse_steam_record(value: &Value) -> Result<SteamCacheRecord, String> {
    #[derive(Debug, Deserialize)]
    struct RawRecord {
        #[serde(default)]
        discord_id: String,
        #[serde(default)]
        steam_id64: String,
        last_display_name: Option<String>,
        last_status: Option<String>,
        first_seen_at: Option<String>,
        last_seen_in_sheet_at: Option<String>,
        last_seen_active_at: Option<String>,
    }

    let raw: RawRecord = serde_json::from_value(value.clone()).map_err(|err| err.to_string())?;
    Ok(SteamCacheRecord {
        discord_id: raw.discord_id,
        steam_id64: raw.steam_id64,
        last_display_name: raw.last_display_name,
        last_status: raw.last_status,
        first_seen_at: raw.first_seen_at,
        last_seen_in_sheet_at: raw.last_seen_in_sheet_at,
        last_seen_active_at: raw.last_seen_active_at,
    })
}

fn read_required_message_id(
    data_dir: &Path,
    file_name: &str,
    label: &str,
    report: &mut Report,
) -> Option<u64> {
    let path = data_dir.join(file_name);
    if !path.is_file() {
        report.fail(
            "clanlist",
            format!("missing required message ID JSON file: {}", path.display()),
        );
        return None;
    }
    match fs::read_to_string(&path) {
        Ok(text) => match parse_message_id_json(&text) {
            Ok(message_id) => {
                report.ok("clanlist", format!("{label} message id = {message_id}"));
                Some(message_id)
            }
            Err(message) => {
                report.fail(
                    "clanlist",
                    format!("invalid JSON in {}: {message}", path.display()),
                );
                None
            }
        },
        Err(err) => {
            report.fail(
                "clanlist",
                format!("failed to read {}: {err}", path.display()),
            );
            None
        }
    }
}

fn merge_relevant_config_status(load: &ConfigLoad, report: &mut Report) {
    let mut emitted = false;
    for item in &load.report.items {
        if item.severity == xiii_core::Severity::Fail {
            emitted = true;
            report.push(
                item.severity,
                "config",
                format!("{} {}", item.scope, item.message),
            );
        }
    }
    if !emitted && !load.report.has_failures() {
        report.ok("config", "unified env loaded and validated");
    }
}

fn validate_required_env(load: &ConfigLoad, report: &mut Report) {
    for name in [
        "LEGACY_CLANLIST_DATA_DIR",
        "CLANLIST_MAIN_CHANNEL_ID",
        "CLANLIST_ADMIN_CHANNEL_ID",
        "CLANLIST_STEAM_CHANNEL_ID",
        "CLANLIST_MAIN_ROLE_IDS",
        "CLANLIST_ADMIN_ROLE_IDS",
        "CLANLIST_STEAM_ACTIVE_ROLE_ID",
    ] {
        match load.entries.iter().find(|entry| entry.name == name) {
            Some(entry) if entry.value != "<MISSING>" && entry.value != "<EMPTY>" => {}
            Some(entry) => report.fail(
                "config",
                format!(
                    "{name} is required for clanlist-preview but is {}",
                    entry.value
                ),
            ),
            None => report.fail(
                "config",
                format!("{name} is required for clanlist-preview but was not reported"),
            ),
        }
    }
}

fn collect_snowflakes(value: &Value) -> Vec<u64> {
    let mut ids = Vec::new();
    collect_snowflakes_inner(value, &mut ids);
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn collect_snowflakes_inner(value: &Value, ids: &mut Vec<u64>) {
    match value {
        Value::Number(number) => {
            if let Some(id) = number.as_u64() {
                if looks_like_snowflake(id) {
                    ids.push(id);
                }
            }
        }
        Value::String(text) => {
            if let Ok(id) = text.parse::<u64>() {
                if looks_like_snowflake(id) {
                    ids.push(id);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_snowflakes_inner(item, ids);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_snowflakes_inner(item, ids);
            }
        }
        _ => {}
    }
}

fn looks_like_snowflake(id: u64) -> bool {
    let len = id.to_string().len();
    (17..=20).contains(&len)
}

const OFFLINE_MODE: &str = "offline preview from legacy JSON/cache only";
const OFFLINE_NOTE: &str =
    "Does not query Discord members or Google Sheets; main/admin roster member lists are not final.";
const DISCORD_SNAPSHOT_MODE: &str =
    "read-only Discord clanlist snapshot from guild roles/members and legacy JSON/cache";
const DISCORD_SNAPSHOT_NOTE: &str =
    "Discord HTTP is used only for read-only guild roles/members; no Discord writes, Google calls, schedulers, migrations, or legacy JSON writes are performed.";
const RENDER_PREVIEW_MODE: &str =
    "read-only Clanlist render parity preview from Discord roles/members and legacy JSON/cache";
const WRITE_PLAN_MODE: &str =
    "dry-run Clanlist write plan from read-only render preview; no writes are executed";
const TARGET_MESSAGE_CHECK_MODE: &str =
    "read-only Clanlist target message verification through exact Discord GET message calls";
const BOOTSTRAP_NEW_PANELS_MODE: &str =
    "fresh Clanlist panel bootstrap; creates new messages only after explicit confirmation";
const UPDATE_PANELS_MODE: &str =
    "manual Clanlist panel update; edits only fresh_bootstrap messages from Superbot state";
const MAIN_PANEL_TITLE: &str =
    "\u{0421}\u{043f}\u{0438}\u{0441}\u{043e}\u{043a} \u{0443}\u{0447}\u{0430}\u{0441}\u{0442}\u{043d}\u{0438}\u{043a}\u{043e}\u{0432} XIII";
const ADMIN_PANEL_TITLE: &str =
    "\u{0410}\u{0434}\u{043c}\u{0438}\u{043d}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{0438}\u{0432}\u{043d}\u{044b}\u{0439} \u{0441}\u{043e}\u{0441}\u{0442}\u{0430}\u{0432} XIII";
const STEAM_PANEL_TITLE: &str = "\u{0421}\u{043f}\u{0438}\u{0441}\u{043e}\u{043a} Steam ID XIII";
const MAIN_PANEL_MARKER_URL: &str = "https://local.discord-roster-bot/panel/main";
const ADMIN_PANEL_MARKER_URL: &str = "https://local.discord-roster-bot/panel/admin";
const STEAM_PANEL_MARKER_URL: &str = "https://local.discord-roster-bot/panel/steam";
const EMBED_COLOR_HEX: &str = "#0066FF";
const RU_TOTAL_MEMBERS: &str =
    "\u{041a}\u{043e}\u{043b}\u{0438}\u{0447}\u{0435}\u{0441}\u{0442}\u{0432}\u{043e} \u{0443}\u{0447}\u{0430}\u{0441}\u{0442}\u{043d}\u{0438}\u{043a}\u{043e}\u{0432}";
const RU_ACTIVE_MEMBERS: &str =
    "\u{0414}\u{0435}\u{0439}\u{0441}\u{0442}\u{0432}\u{0443}\u{044e}\u{0449}\u{0438}\u{0435} \u{0443}\u{0447}\u{0430}\u{0441}\u{0442}\u{043d}\u{0438}\u{043a}\u{0438}";
const RU_EXCLUDED_MEMBERS: &str =
    "\u{0418}\u{0441}\u{043a}\u{043b}\u{044e}\u{0447}\u{0435}\u{043d}\u{043d}\u{044b}\u{0435} \u{0443}\u{0447}\u{0430}\u{0441}\u{0442}\u{043d}\u{0438}\u{043a}\u{0438}";
const RU_CONTINUATION: &str =
    "\u{043f}\u{0440}\u{043e}\u{0434}\u{043e}\u{043b}\u{0436}\u{0435}\u{043d}\u{0438}\u{0435}";
const RU_NO_RECORDS: &str =
    "\u{041d}\u{0435}\u{0442} \u{0437}\u{0430}\u{043f}\u{0438}\u{0441}\u{0435}\u{0439}.";
const RU_UPDATED_FOOTER_TEMPLATE: &str =
    "\u{041e}\u{0431}\u{043d}\u{043e}\u{0432}\u{043b}\u{0435}\u{043d}\u{043e}: <dd.mm.yyyy HH:MM>";
const MAX_EMBEDS_PER_MESSAGE: usize = 10;
const MAX_FIELD_VALUE_LENGTH: usize = 1024;
const STEAM_BLOCK_VALUE_LENGTH: usize = 3800;
const TEXT_MEMBER_PREVIEW_LIMIT: usize = 20;

#[cfg(test)]
mod tests {
    use super::{
        apply_bootstrap_outcomes, apply_successful_update_to_state, apply_update_outcomes,
        build_bootstrap_new_panels, build_discord_readonly_snapshot, build_panel_state,
        build_preview, build_render_preview, build_target_message_check, build_update_panels,
        build_write_plan, parse_message_id_json, parse_panel_state_json, parse_steam_cache_count,
        render_bootstrap_new_panels_json, render_bootstrap_new_panels_text,
        render_discord_snapshot_json, render_discord_snapshot_text, render_json,
        render_render_preview_json, render_render_preview_text,
        render_render_preview_text_with_options, render_target_message_check_json,
        render_target_message_check_text, render_text, render_update_panels_json,
        render_update_panels_text, render_write_plan_json, render_write_plan_text,
        target_message_targets_from_preview, update_targets_from_state,
        validate_bootstrap_payloads, validate_panel_state, validate_update_payloads,
        BootstrapOperationOutcome, ClanlistPanelState, ClanlistPanelStateRenderSummary,
        ClanlistPanelStateTarget, ClanlistPreviewOptions, DiscordMemberSnapshotInput,
        DiscordRoleSnapshotInput, SteamPreviewMode, TargetMessageObservationInput,
        UpdateOperationOutcome,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use xiii_config::SuperbotConfig;
    use xiii_core::Report;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn parses_message_id_json_array() {
        assert_eq!(
            parse_message_id_json("[1498766315299799185]").unwrap(),
            1_498_766_315_299_799_185
        );
    }

    #[test]
    fn parses_steam_cache_count_from_records_map() {
        let cache = r#"{"records":{"1":{"discord_id":"1","steam_id64":"2"},"3":{"discord_id":"3","steam_id64":"4"}}}"#;
        assert_eq!(parse_steam_cache_count(cache).unwrap(), 2);
    }

    #[test]
    fn missing_steam_cache_warns_in_auto_mode() {
        let dir = fixture_dir("missing_steam_cache_warns");
        write_required_message_files(&dir);
        let env = env_for_dir(&dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();

        let result = build_preview(&load, ClanlistPreviewOptions::default());

        assert!(!result.has_critical_failures());
        assert_eq!(result.report.counts().warn, 1);
        cleanup_dir(dir);
    }

    #[test]
    fn constructs_preview_model_from_legacy_json() {
        let dir = fixture_dir("constructs_preview");
        write_required_message_files(&dir);
        fs::write(
            dir.join("steam_roster_cache.json"),
            r#"{"records":{"973660882242519150":{"discord_id":"973660882242519150","steam_id64":"76561199861742815","last_display_name":"aglix.","last_status":"active"}}}"#,
        )
        .unwrap();
        let env = env_for_dir(&dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();

        let result = build_preview(&load, ClanlistPreviewOptions::default());
        let model = result.model.as_ref().unwrap();

        assert_eq!(model.targets.main.channel_id, 1_498_762_828_666_896_535);
        assert_eq!(model.targets.main.message_id, 1_498_766_315_299_799_185);
        assert_eq!(model.targets.steam.cached_records, 1);
        cleanup_dir(dir);
    }

    #[test]
    fn text_output_does_not_contain_secrets() {
        let dir = fixture_dir("text_no_secrets");
        write_required_message_files(&dir);
        fs::write(dir.join("steam_roster_cache.json"), r#"{"records":{}}"#).unwrap();
        let env = env_for_dir(&dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        let result = build_preview(&load, ClanlistPreviewOptions::default());

        let text = render_text(&result);

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(text.contains("offline preview"));
        cleanup_dir(dir);
    }

    #[test]
    fn json_output_is_valid_json() {
        let dir = fixture_dir("json_valid");
        write_required_message_files(&dir);
        fs::write(dir.join("steam_roster_cache.json"), r#"{"records":{}}"#).unwrap();
        let env = env_for_dir(&dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        let result = build_preview(
            &load,
            ClanlistPreviewOptions {
                steam: SteamPreviewMode::Include,
            },
        );

        let json = render_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["safety"]["discord_http"], false);
        assert!(parsed["targets"]["main"]["message_id"].is_number());
        cleanup_dir(dir);
    }

    #[test]
    fn snapshot_groups_members_by_configured_roles() {
        let dir = fixture_dir("snapshot_grouping");
        let result = snapshot_fixture(&dir, true, roles_fixture(), members_fixture());
        let model = result.model.as_ref().unwrap();

        assert_eq!(model.main_roles[0].member_count, Some(2));
        assert_eq!(model.main_roles[1].member_count, Some(1));
        assert_eq!(model.admin_roles[1].member_count, Some(1));
        assert_eq!(model.member_count, Some(4));
        cleanup_dir(dir);
    }

    #[test]
    fn snapshot_warns_for_missing_configured_roles() {
        let dir = fixture_dir("snapshot_missing_role");
        let mut roles = roles_fixture();
        roles.retain(|role| role.id != 1_498_022_112_131_289_216);

        let result = snapshot_fixture(&dir, true, roles, members_fixture());

        assert!(result
            .model
            .as_ref()
            .unwrap()
            .missing_configured_roles
            .contains(&1_498_022_112_131_289_216));
        assert!(result.report.counts().warn >= 1);
        cleanup_dir(dir);
    }

    #[test]
    fn snapshot_detects_members_with_multiple_configured_roles() {
        let dir = fixture_dir("snapshot_multiple_roles");
        let result = snapshot_fixture(&dir, true, roles_fixture(), members_fixture());
        let model = result.model.as_ref().unwrap();

        assert_eq!(model.members_with_multiple_configured_main_roles.len(), 1);
        assert_eq!(
            model.members_with_multiple_configured_main_roles[0].user_id,
            100
        );
        cleanup_dir(dir);
    }

    #[test]
    fn snapshot_json_output_is_valid_json() {
        let dir = fixture_dir("snapshot_json_valid");
        let result = snapshot_fixture(&dir, true, roles_fixture(), members_fixture());

        let json = render_discord_snapshot_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["safety"]["discord_http"], true);
        assert!(parsed["targets"]["main"]["message_id"].is_number());
        cleanup_dir(dir);
    }

    #[test]
    fn snapshot_text_and_json_do_not_contain_secrets() {
        let dir = fixture_dir("snapshot_no_secrets");
        let result = snapshot_fixture(&dir, true, roles_fixture(), members_fixture());

        let text = render_discord_snapshot_text(&result);
        let json = render_discord_snapshot_json(&result).unwrap();

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(!json.contains("super-secret-token"));
        assert!(!json.contains("sheet-secret"));
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_groups_main_roster_in_legacy_order() {
        let dir = fixture_dir("render_main_order");
        let result = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            r#"{"records":{}}"#,
        );
        let model = result.model.as_ref().unwrap();

        assert_eq!(model.main_panel.role_sections.len(), 1);
        assert_eq!(model.main_panel.role_sections[0].role_name, "Council");
        assert_eq!(
            model.main_panel.role_sections[0].members[0].display_name,
            "Dual Main"
        );
        assert_eq!(
            model.main_panel.role_sections[0].members[1].display_name,
            "Single Main"
        );
        assert!(model
            .main_panel
            .omitted_role_ids
            .contains(&1_498_022_112_131_289_216));
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_groups_admin_roster_in_legacy_order() {
        let dir = fixture_dir("render_admin_order");
        let result = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            r#"{"records":{}}"#,
        );
        let model = result.model.as_ref().unwrap();

        assert_eq!(model.admin_panel.role_sections[0].role_name, "Council");
        assert_eq!(model.admin_panel.role_sections[1].role_name, "Support");
        assert_eq!(
            model.admin_panel.role_sections[1].members[0].display_name,
            "Admin Only"
        );
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_splits_steam_cache_active_excluded_and_unknown() {
        let dir = fixture_dir("render_steam_split");
        let cache = r#"{"records":{
            "100":{"discord_id":"100","steam_id64":"76561198000000100","last_display_name":"Dual Main"},
            "103":{"discord_id":"103","steam_id64":"76561198000000103","last_display_name":"No Roster"},
            "999":{"discord_id":"999","steam_id64":"76561198000000999","last_display_name":"Gone"}
        }}"#;
        let mut members = members_fixture();
        members[0].role_ids.push(1_498_022_112_114_249_827);
        let result = render_fixture(&dir, true, roles_fixture(), members, cache);
        let steam = result.model.as_ref().unwrap().steam_panel.as_ref().unwrap();

        assert_eq!(steam.active_count, Some(1));
        assert_eq!(steam.excluded_count, Some(2));
        assert_eq!(steam.unknown_member_count, 1);
        assert_eq!(steam.active_block.entries[0].discord_id, "100");
        assert_eq!(steam.unknown_member_entries[0].status, "unknown_member");
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_uses_legacy_display_name_sorting() {
        let dir = fixture_dir("render_sorting");
        let members = vec![
            DiscordMemberSnapshotInput {
                user_id: 201,
                display_name: "bravo".to_owned(),
                role_ids: vec![1_498_022_112_131_289_217],
            },
            DiscordMemberSnapshotInput {
                user_id: 200,
                display_name: "Alpha".to_owned(),
                role_ids: vec![1_498_022_112_131_289_217],
            },
        ];
        let result = render_fixture(&dir, true, roles_fixture(), members, r#"{"records":{}}"#);
        let members = &result.model.as_ref().unwrap().main_panel.role_sections[0].members;

        assert_eq!(members[0].display_name, "Alpha");
        assert_eq!(members[1].display_name, "bravo");
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_warns_for_missing_role() {
        let dir = fixture_dir("render_missing_role");
        let mut roles = roles_fixture();
        roles.retain(|role| role.id != 1_498_022_112_131_289_216);
        let result = render_fixture(&dir, true, roles, members_fixture(), r#"{"records":{}}"#);

        assert!(result
            .report
            .items
            .iter()
            .any(|item| item.message.contains("configured role missing")));
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_warns_for_multi_rank_member() {
        let dir = fixture_dir("render_multi_rank");
        let result = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            r#"{"records":{}}"#,
        );

        assert!(result
            .report
            .items
            .iter()
            .any(|item| item.message.contains("multiple configured rank roles")));
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_text_truncates_large_sections() {
        let dir = fixture_dir("render_text_truncates");
        let members = (0..25)
            .map(|index| DiscordMemberSnapshotInput {
                user_id: 10_000 + index,
                display_name: format!("Member {index:02}"),
                role_ids: vec![1_498_022_112_131_289_217],
            })
            .collect::<Vec<_>>();
        let result = render_fixture(&dir, true, roles_fixture(), members, r#"{"records":{}}"#);
        let text = render_render_preview_text(&result);

        assert!(text.contains("... 5 more omitted"));
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_text_custom_max_members_per_section() {
        let dir = fixture_dir("render_text_custom_max");
        let members = (0..25)
            .map(|index| DiscordMemberSnapshotInput {
                user_id: 20_000 + index,
                display_name: format!("Member {index:02}"),
                role_ids: vec![1_498_022_112_131_289_217],
            })
            .collect::<Vec<_>>();
        let result = render_fixture(&dir, true, roles_fixture(), members, r#"{"records":{}}"#);
        let text = render_render_preview_text_with_options(
            &result,
            super::RenderTextOptions {
                max_members_per_section: 50,
            },
        );

        assert!(!text.contains("more omitted"));
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_json_output_is_valid_json() {
        let dir = fixture_dir("render_json_valid");
        let result = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            r#"{"records":{}}"#,
        );

        let json = render_render_preview_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["safety"]["discord_http"], true);
        assert!(parsed["main_panel"]["header"]["title"].is_string());
        cleanup_dir(dir);
    }

    #[test]
    fn render_preview_text_and_json_do_not_contain_secrets() {
        let dir = fixture_dir("render_no_secrets");
        let result = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            r#"{"records":{}}"#,
        );

        let text = render_render_preview_text(&result);
        let json = render_render_preview_json(&result).unwrap();

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(!json.contains("super-secret-token"));
        assert!(!json.contains("sheet-secret"));
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_generates_exactly_three_operations() {
        let dir = fixture_dir("write_plan_three_ops");
        let render = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );

        let result = build_write_plan(render, Report::new());

        let model = result.model.as_ref().unwrap();
        assert_eq!(model.planned_operations.len(), 3);
        assert_eq!(model.planned_operations[0].panel_name, "main");
        assert_eq!(model.planned_operations[1].panel_name, "admin");
        assert_eq!(model.planned_operations[2].panel_name, "steam");
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_detects_duplicate_message_ids() {
        let dir = fixture_dir("write_plan_duplicate_ids");
        let mut render = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        let model = render.model.as_mut().unwrap();
        model.admin_panel.target.message_id = model.main_panel.target.message_id;

        let result = build_write_plan(render, Report::new());

        assert!(result
            .report
            .items
            .iter()
            .any(|item| item.message.contains("duplicate target message IDs")));
        assert!(result.has_critical_failures());
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_fails_missing_message_id() {
        let dir = fixture_dir("write_plan_missing_id");
        let mut render = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        render.model.as_mut().unwrap().main_panel.target.message_id = 0;

        let result = build_write_plan(render, Report::new());

        assert!(result
            .report
            .items
            .iter()
            .any(|item| item.message.contains("target main message id = 0")));
        assert!(result.has_critical_failures());
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_json_output_is_valid() {
        let dir = fixture_dir("write_plan_json_valid");
        let render = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        let result = build_write_plan(render, Report::new());

        let json = render_write_plan_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["planned_operations"].as_array().unwrap().len(), 3);
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_output_does_not_contain_secrets() {
        let dir = fixture_dir("write_plan_no_secrets");
        let render = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        let result = build_write_plan(render, Report::new());

        let text = render_write_plan_text(&result);
        let json = render_write_plan_json(&result).unwrap();

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(!json.contains("super-secret-token"));
        assert!(!json.contains("sheet-secret"));
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_operations_are_never_allowed() {
        let dir = fixture_dir("write_plan_allowed_false");
        let render = render_fixture(
            &dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        let result = build_write_plan(render, Report::new());

        assert!(result
            .model
            .as_ref()
            .unwrap()
            .planned_operations
            .iter()
            .all(|operation| !operation.allowed));
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_list_has_exactly_three_targets() {
        let dir = fixture_dir("target_message_three_targets");
        write_required_message_files(&dir);
        fs::write(dir.join("steam_roster_cache.json"), steam_cache_fixture()).unwrap();
        let env = env_for_dir(&dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        let preview = build_preview(&load, ClanlistPreviewOptions::default());

        let targets = target_message_targets_from_preview(preview.model.as_ref().unwrap());

        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].panel_name, "main");
        assert_eq!(targets[1].panel_name, "admin");
        assert_eq!(targets[2].panel_name, "steam");
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_author_match_is_editable_candidate() {
        let dir = fixture_dir("target_message_author_match");
        let result = target_message_check_fixture(&dir, 42, all_target_observations(42));

        let model = result.model.as_ref().unwrap();
        assert!(model.all_targets_editable_candidates);
        assert!(model
            .target_checks
            .iter()
            .all(|check| check.editable_by_current_bot));
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_author_mismatch_is_failure() {
        let dir = fixture_dir("target_message_author_mismatch");
        let mut observations = all_target_observations(42);
        observations[0].author_id = Some(41);
        let result = target_message_check_fixture(&dir, 42, observations);

        let main = result
            .model
            .as_ref()
            .unwrap()
            .target_checks
            .iter()
            .find(|check| check.panel_name == "main")
            .unwrap();
        assert!(!main.editable_by_current_bot);
        assert!(result.has_critical_failures());
        assert!(result.report.items.iter().any(|item| item
            .message
            .contains("Discord bots can edit only their own messages")));
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_missing_message_is_failure() {
        let dir = fixture_dir("target_message_missing");
        let mut observations = all_target_observations(42);
        observations[0].exists = false;
        observations[0].failure_reason = Some("HTTP 404 Not Found".to_owned());
        let result = target_message_check_fixture(&dir, 42, observations);

        let main = result
            .model
            .as_ref()
            .unwrap()
            .target_checks
            .iter()
            .find(|check| check.panel_name == "main")
            .unwrap();
        assert!(!main.message_exists);
        assert!(result.has_critical_failures());
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_title_match_and_mismatch_are_reported() {
        let dir = fixture_dir("target_message_title_mismatch");
        let mut observations = all_target_observations(42);
        observations[0].first_embed_title = Some("Different title".to_owned());
        let result = target_message_check_fixture(&dir, 42, observations);

        let main = result
            .model
            .as_ref()
            .unwrap()
            .target_checks
            .iter()
            .find(|check| check.panel_name == "main")
            .unwrap();
        assert_eq!(main.title_roughly_matches, Some(false));
        assert!(result
            .report
            .items
            .iter()
            .any(|item| item.message.contains("first embed title differs")));
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_json_output_is_valid() {
        let dir = fixture_dir("target_message_json_valid");
        let result = target_message_check_fixture(&dir, 42, all_target_observations(42));

        let json = render_target_message_check_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["target_checks"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["safety"]["discord_http"], true);
        cleanup_dir(dir);
    }

    #[test]
    fn target_message_output_does_not_contain_secrets() {
        let dir = fixture_dir("target_message_no_secrets");
        let result = target_message_check_fixture(&dir, 42, all_target_observations(42));

        let text = render_target_message_check_text(&result);
        let json = render_target_message_check_json(&result).unwrap();

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(!json.contains("super-secret-token"));
        assert!(!json.contains("sheet-secret"));
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_dry_run_does_not_create_messages_or_state() {
        let dir = fixture_dir("bootstrap_dry_run");
        let result = bootstrap_fixture(&dir, true);
        let model = result.model.as_ref().unwrap();

        assert!(model.dry_run);
        assert!(model.operations.iter().all(|operation| {
            operation.status == "dry_run" && operation.new_message_id.is_none()
        }));
        assert!(model.state_file_path.is_none());
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_payload_validation_rejects_more_than_ten_embeds() {
        let dir = fixture_dir("bootstrap_too_many_embeds");
        let result = bootstrap_fixture(&dir, true);
        let mut payloads = result.model.as_ref().unwrap().payloads.clone();
        let extra = payloads[0].embeds[0].clone();
        while payloads[0].embeds.len() <= super::MAX_EMBEDS_PER_MESSAGE {
            payloads[0].embeds.push(extra.clone());
        }
        let mut report = Report::new();

        validate_bootstrap_payloads(&payloads, &mut report);

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("Discord message limit")));
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_payloads_disable_allowed_mentions() {
        let dir = fixture_dir("bootstrap_allowed_mentions");
        let result = bootstrap_fixture(&dir, true);

        assert!(result
            .model
            .as_ref()
            .unwrap()
            .payloads
            .iter()
            .all(|payload| payload.content.is_none() && payload.allowed_mentions_disabled));
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_state_json_includes_all_three_panel_ids() {
        let dir = fixture_dir("bootstrap_state_json");
        let mut result = bootstrap_fixture(&dir, false);
        apply_bootstrap_outcomes(
            &mut result,
            vec![
                BootstrapOperationOutcome {
                    panel_name: "main",
                    new_message_id: Some(9001),
                    failure_reason: None,
                },
                BootstrapOperationOutcome {
                    panel_name: "admin",
                    new_message_id: Some(9002),
                    failure_reason: None,
                },
                BootstrapOperationOutcome {
                    panel_name: "steam",
                    new_message_id: Some(9003),
                    failure_reason: None,
                },
            ],
        );

        let state =
            build_panel_state(result.model.as_ref().unwrap(), "2026-05-09T10:00:00Z").unwrap();
        let json = serde_json::to_string(&state).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["main"]["message_id"], 9001);
        assert_eq!(parsed["admin"]["message_id"], 9002);
        assert_eq!(parsed["steam"]["message_id"], 9003);
        assert_eq!(parsed["source"], "fresh_bootstrap");
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_partial_creation_report_includes_created_ids_and_failure() {
        let dir = fixture_dir("bootstrap_partial_report");
        let mut result = bootstrap_fixture(&dir, false);
        apply_bootstrap_outcomes(
            &mut result,
            vec![
                BootstrapOperationOutcome {
                    panel_name: "main",
                    new_message_id: Some(9001),
                    failure_reason: None,
                },
                BootstrapOperationOutcome {
                    panel_name: "admin",
                    new_message_id: None,
                    failure_reason: Some("HTTP 500".to_owned()),
                },
            ],
        );

        let text = render_bootstrap_new_panels_text(&result);

        assert!(text.contains("9001"));
        assert!(text.contains("HTTP 500"));
        assert!(result.has_critical_failures());
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_json_output_is_valid() {
        let dir = fixture_dir("bootstrap_json_valid");
        let result = bootstrap_fixture(&dir, true);

        let json = render_bootstrap_new_panels_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["payloads"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["safety"]["allowed_mentions_disabled"], true);
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_output_does_not_contain_secrets() {
        let dir = fixture_dir("bootstrap_no_secrets");
        let result = bootstrap_fixture(&dir, true);

        let text = render_bootstrap_new_panels_text(&result);
        let json = render_bootstrap_new_panels_json(&result).unwrap();

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(!json.contains("super-secret-token"));
        assert!(!json.contains("sheet-secret"));
        cleanup_dir(dir);
    }

    #[test]
    fn update_state_parser_accepts_valid_fresh_bootstrap_state() {
        let state = parse_panel_state_json(&valid_panel_state_json()).unwrap();

        assert_eq!(state.source, "fresh_bootstrap");
        assert_eq!(state.main.message_id, 1_502_618_001_881_436_320);
        assert_eq!(state.bot_user_id, 42);
    }

    #[test]
    fn update_state_validation_rejects_missing_panel_ids() {
        let mut state = valid_panel_state();
        state.main.message_id = 0;
        let mut report = Report::new();

        validate_panel_state(&state, 1_498_022_112_114_249_819, &mut report);

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("main message_id is zero")));
    }

    #[test]
    fn update_state_validation_rejects_duplicate_message_ids() {
        let mut state = valid_panel_state();
        state.admin.message_id = state.main.message_id;
        let mut report = Report::new();

        validate_panel_state(&state, 1_498_022_112_114_249_819, &mut report);

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("duplicated")));
    }

    #[test]
    fn update_state_bot_user_mismatch_is_failure() {
        let dir = fixture_dir("update_bot_mismatch");
        let result = update_fixture(
            &dir,
            true,
            valid_panel_state(),
            999,
            all_update_observations(42),
        );

        assert!(result.has_critical_failures());
        assert!(result
            .report
            .items
            .iter()
            .any(|item| item.message.contains("does not match state bot_user_id")));
        cleanup_dir(dir);
    }

    #[test]
    fn update_target_author_mismatch_is_failure_before_edit() {
        let dir = fixture_dir("update_author_mismatch");
        let mut observations = all_update_observations(42);
        observations[0].author_id = Some(41);
        let result = update_fixture(&dir, true, valid_panel_state(), 42, observations);

        let main = result
            .model
            .as_ref()
            .unwrap()
            .target_checks
            .iter()
            .find(|check| check.panel_name == "main")
            .unwrap();
        assert!(!main.editable_by_current_bot);
        assert!(result.has_critical_failures());
        cleanup_dir(dir);
    }

    #[test]
    fn update_dry_run_produces_three_planned_operations_and_no_state_update() {
        let dir = fixture_dir("update_dry_run");
        let result = update_fixture(
            &dir,
            true,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );
        let model = result.model.as_ref().unwrap();

        assert_eq!(model.operations.len(), 3);
        assert!(model.operations.iter().all(|operation| {
            operation.status == "dry_run"
                && !operation.allowed
                && operation.edited_message_id.is_none()
        }));
        assert!(model.state_updated_path.is_none());
        cleanup_dir(dir);
    }

    #[test]
    fn update_execute_model_edits_exactly_three_operations() {
        let dir = fixture_dir("update_execute_model");
        let mut result = update_fixture(
            &dir,
            false,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );

        apply_update_outcomes(
            &mut result,
            vec![
                UpdateOperationOutcome {
                    panel_name: "main",
                    edited_message_id: Some(1_502_618_001_881_436_320),
                    failure_reason: None,
                },
                UpdateOperationOutcome {
                    panel_name: "admin",
                    edited_message_id: Some(1_502_618_004_545_077_269),
                    failure_reason: None,
                },
                UpdateOperationOutcome {
                    panel_name: "steam",
                    edited_message_id: Some(1_502_618_005_841_117_215),
                    failure_reason: None,
                },
            ],
        );

        assert_eq!(
            result
                .model
                .as_ref()
                .unwrap()
                .operations
                .iter()
                .filter(|operation| operation.status == "edited")
                .count(),
            3
        );
        cleanup_dir(dir);
    }

    #[test]
    fn update_payloads_disable_allowed_mentions() {
        let dir = fixture_dir("update_allowed_mentions");
        let result = update_fixture(
            &dir,
            true,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );

        assert!(result
            .model
            .as_ref()
            .unwrap()
            .payloads
            .iter()
            .all(|payload| payload.content.is_none() && payload.allowed_mentions_disabled));
        cleanup_dir(dir);
    }

    #[test]
    fn update_payload_validation_rejects_more_than_ten_embeds() {
        let dir = fixture_dir("update_too_many_embeds");
        let result = update_fixture(
            &dir,
            true,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );
        let mut payloads = result.model.as_ref().unwrap().payloads.clone();
        let extra = payloads[0].embeds[0].clone();
        while payloads[0].embeds.len() <= super::MAX_EMBEDS_PER_MESSAGE {
            payloads[0].embeds.push(extra.clone());
        }
        let mut report = Report::new();

        validate_update_payloads(&payloads, &mut report);

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("Discord message limit")));
        cleanup_dir(dir);
    }

    #[test]
    fn update_partial_report_includes_edited_ids_and_failure() {
        let dir = fixture_dir("update_partial_report");
        let mut result = update_fixture(
            &dir,
            false,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );
        apply_update_outcomes(
            &mut result,
            vec![
                UpdateOperationOutcome {
                    panel_name: "main",
                    edited_message_id: Some(1_502_618_001_881_436_320),
                    failure_reason: None,
                },
                UpdateOperationOutcome {
                    panel_name: "admin",
                    edited_message_id: None,
                    failure_reason: Some("HTTP 500".to_owned()),
                },
            ],
        );

        let text = render_update_panels_text(&result);

        assert!(text.contains("1502618001881436320"));
        assert!(text.contains("HTTP 500"));
        assert!(result.has_critical_failures());
        cleanup_dir(dir);
    }

    #[test]
    fn update_state_json_includes_last_update_and_message_ids() {
        let dir = fixture_dir("update_state_json");
        let mut state = valid_panel_state();
        let mut result =
            update_fixture(&dir, false, state.clone(), 42, all_update_observations(42));
        apply_update_outcomes(
            &mut result,
            vec![
                UpdateOperationOutcome {
                    panel_name: "main",
                    edited_message_id: Some(1_502_618_001_881_436_320),
                    failure_reason: None,
                },
                UpdateOperationOutcome {
                    panel_name: "admin",
                    edited_message_id: Some(1_502_618_004_545_077_269),
                    failure_reason: None,
                },
                UpdateOperationOutcome {
                    panel_name: "steam",
                    edited_message_id: Some(1_502_618_005_841_117_215),
                    failure_reason: None,
                },
            ],
        );

        apply_successful_update_to_state(
            &mut state,
            result.model.as_ref().unwrap(),
            "2026-05-09T10:05:00Z",
        )
        .unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();

        assert_eq!(parsed["last_updated_at_utc"], "2026-05-09T10:05:00Z");
        assert_eq!(
            parsed["last_successful_update_message_ids"]["steam"],
            1_502_618_005_841_117_215u64
        );
        cleanup_dir(dir);
    }

    #[test]
    fn update_json_output_is_valid() {
        let dir = fixture_dir("update_json_valid");
        let result = update_fixture(
            &dir,
            true,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );

        let json = render_update_panels_json(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["operations"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["safety"]["discord_message_creates"], false);
        cleanup_dir(dir);
    }

    #[test]
    fn update_output_does_not_contain_secrets() {
        let dir = fixture_dir("update_no_secrets");
        let result = update_fixture(
            &dir,
            true,
            valid_panel_state(),
            42,
            all_update_observations(42),
        );

        let text = render_update_panels_text(&result);
        let json = render_update_panels_json(&result).unwrap();

        assert!(!text.contains("super-secret-token"));
        assert!(!text.contains("sheet-secret"));
        assert!(!json.contains("super-secret-token"));
        assert!(!json.contains("sheet-secret"));
        cleanup_dir(dir);
    }

    fn snapshot_fixture(
        dir: &Path,
        include_members: bool,
        roles: Vec<DiscordRoleSnapshotInput>,
        members: Vec<DiscordMemberSnapshotInput>,
    ) -> super::ClanlistDiscordSnapshotResult {
        write_required_message_files(dir);
        fs::write(dir.join("steam_roster_cache.json"), r#"{"records":{}}"#).unwrap();
        let env = env_for_dir(dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        let preview = build_preview(&load, ClanlistPreviewOptions::default());
        let mut discord_report = Report::new();
        discord_report.ok("discord", "connected to Discord read-only");
        discord_report.ok("discord", "roles fetched = 4");
        if include_members {
            discord_report.ok("discord", "members fetched = 4");
        }
        build_discord_readonly_snapshot(
            preview,
            1_498_022_112_114_249_819,
            roles,
            include_members.then_some(members),
            include_members,
            discord_report,
        )
    }

    fn target_message_check_fixture(
        dir: &Path,
        current_bot_user_id: u64,
        observations: Vec<TargetMessageObservationInput>,
    ) -> super::ClanlistTargetMessageCheckResult {
        write_required_message_files(dir);
        fs::write(dir.join("steam_roster_cache.json"), steam_cache_fixture()).unwrap();
        let env = env_for_dir(dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        let preview = build_preview(&load, ClanlistPreviewOptions::default());
        let mut discord_report = Report::new();
        discord_report.ok("discord", "connected to Discord read-only");
        build_target_message_check(preview, current_bot_user_id, observations, discord_report)
    }

    fn bootstrap_fixture(dir: &Path, dry_run: bool) -> super::ClanlistBootstrapNewPanelsResult {
        let render = render_fixture(
            dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        let preview_targets = render_preview_targets(dir);
        build_bootstrap_new_panels(
            render,
            Some(preview_targets),
            1_502_415_028_685_504_675,
            dry_run,
            "2026-05-09T10:00:00Z",
        )
    }

    fn update_fixture(
        dir: &Path,
        dry_run: bool,
        state: ClanlistPanelState,
        current_bot_user_id: u64,
        observations: Vec<TargetMessageObservationInput>,
    ) -> super::ClanlistUpdatePanelsResult {
        let render = render_fixture(
            dir,
            true,
            roles_fixture(),
            members_fixture(),
            steam_cache_fixture(),
        );
        build_update_panels(
            render,
            state,
            "data/clanlist_panel_state.json",
            current_bot_user_id,
            observations,
            Report::new(),
            dry_run,
            "2026-05-09T10:05:00Z",
        )
    }

    fn valid_panel_state_json() -> String {
        serde_json::to_string(&valid_panel_state()).unwrap()
    }

    fn valid_panel_state() -> ClanlistPanelState {
        ClanlistPanelState {
            created_at_utc: "2026-05-09T10:00:00Z".to_owned(),
            guild_id: 1_498_022_112_114_249_819,
            bot_user_id: 42,
            source: "fresh_bootstrap".to_owned(),
            main: ClanlistPanelStateTarget {
                channel_id: 1_498_762_828_666_896_535,
                message_id: 1_502_618_001_881_436_320,
            },
            admin: ClanlistPanelStateTarget {
                channel_id: 1_498_763_049_102_868_672,
                message_id: 1_502_618_004_545_077_269,
            },
            steam: ClanlistPanelStateTarget {
                channel_id: 1_500_081_418_506_862_754,
                message_id: 1_502_618_005_841_117_215,
            },
            render_summary: ClanlistPanelStateRenderSummary {
                main_total_members: Some(31),
                admin_total_members: Some(7),
                steam_active_records: Some(18),
                steam_excluded_records: Some(1),
                steam_unknown_member_records: Some(0),
            },
            old_legacy_message_ids: super::BootstrapLegacyMessageIds {
                main: 1_498_766_315_299_799_185,
                admin: 1_498_766_321_867_821_218,
                steam: 1_500_086_435_506_683_954,
            },
            warning: "Old Clanlist panels were not edited or deleted by this bootstrap.".to_owned(),
            last_updated_at_utc: None,
            last_update_source: None,
            last_run_mode: None,
            last_render_summary: None,
            last_successful_update_message_ids: None,
        }
    }

    fn all_update_observations(author_id: u64) -> Vec<TargetMessageObservationInput> {
        update_targets_from_state(&valid_panel_state())
            .into_iter()
            .map(|target| {
                target_observation(
                    target.panel_name,
                    target.channel_id,
                    target.message_id,
                    author_id,
                    &target.expected_title,
                )
            })
            .collect()
    }

    fn render_preview_targets(dir: &Path) -> super::ClanlistPreviewTargets {
        let env = env_for_dir(dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        build_preview(&load, ClanlistPreviewOptions::default())
            .model
            .unwrap()
            .targets
    }

    fn all_target_observations(author_id: u64) -> Vec<TargetMessageObservationInput> {
        vec![
            target_observation(
                "main",
                1_498_762_828_666_896_535,
                1_498_766_315_299_799_185,
                author_id,
                super::MAIN_PANEL_TITLE,
            ),
            target_observation(
                "admin",
                1_498_763_049_102_868_672,
                1_498_766_321_867_821_218,
                author_id,
                super::ADMIN_PANEL_TITLE,
            ),
            target_observation(
                "steam",
                1_500_081_418_506_862_754,
                1_500_086_435_506_683_954,
                author_id,
                super::STEAM_PANEL_TITLE,
            ),
        ]
    }

    fn target_observation(
        panel_name: &'static str,
        channel_id: u64,
        message_id: u64,
        author_id: u64,
        title: &str,
    ) -> TargetMessageObservationInput {
        TargetMessageObservationInput {
            panel_name,
            channel_id,
            message_id,
            exists: true,
            failure_reason: None,
            author_id: Some(author_id),
            embed_count: Some(2),
            first_embed_title: Some(title.to_owned()),
            first_embed_footer_text: Some(super::RU_UPDATED_FOOTER_TEMPLATE.to_owned()),
            first_embed_footer_icon_url: Some("https://example.invalid/icon.png".to_owned()),
            first_embed_marker_url: Some("https://local.discord-roster-bot/panel/test".to_owned()),
        }
    }

    fn render_fixture(
        dir: &Path,
        include_members: bool,
        roles: Vec<DiscordRoleSnapshotInput>,
        members: Vec<DiscordMemberSnapshotInput>,
        steam_cache: &str,
    ) -> super::ClanlistRenderPreviewResult {
        write_required_message_files(dir);
        fs::write(dir.join("steam_roster_cache.json"), steam_cache).unwrap();
        let env = env_for_dir(dir, "super-secret-token", "sheet-secret");
        let load = SuperbotConfig::load_from_env_str(&env).unwrap();
        let preview = build_preview(&load, ClanlistPreviewOptions::default());
        let mut discord_report = Report::new();
        discord_report.ok("discord", "connected to Discord read-only");
        discord_report.ok("discord", format!("roles fetched = {}", roles.len()));
        if include_members {
            discord_report.ok("discord", format!("members fetched = {}", members.len()));
        }
        build_render_preview(
            preview,
            1_498_022_112_114_249_819,
            roles,
            include_members.then_some(members),
            include_members,
            discord_report,
        )
    }

    fn roles_fixture() -> Vec<DiscordRoleSnapshotInput> {
        vec![
            DiscordRoleSnapshotInput {
                id: 1_498_022_112_131_289_217,
                name: "Council".to_owned(),
            },
            DiscordRoleSnapshotInput {
                id: 1_498_022_112_131_289_216,
                name: "Officer".to_owned(),
            },
            DiscordRoleSnapshotInput {
                id: 1_498_057_076_151_422_976,
                name: "Support".to_owned(),
            },
            DiscordRoleSnapshotInput {
                id: 1_498_022_112_114_249_827,
                name: "Member".to_owned(),
            },
        ]
    }

    fn members_fixture() -> Vec<DiscordMemberSnapshotInput> {
        vec![
            DiscordMemberSnapshotInput {
                user_id: 100,
                display_name: "Dual Main".to_owned(),
                role_ids: vec![1_498_022_112_131_289_217, 1_498_022_112_131_289_216],
            },
            DiscordMemberSnapshotInput {
                user_id: 101,
                display_name: "Single Main".to_owned(),
                role_ids: vec![1_498_022_112_131_289_217],
            },
            DiscordMemberSnapshotInput {
                user_id: 102,
                display_name: "Admin Only".to_owned(),
                role_ids: vec![1_498_057_076_151_422_976],
            },
            DiscordMemberSnapshotInput {
                user_id: 103,
                display_name: "No Roster".to_owned(),
                role_ids: vec![999],
            },
        ]
    }

    fn steam_cache_fixture() -> &'static str {
        r#"{"records":{"100":{"discord_id":"100","steam_id64":"76561198000000100","last_display_name":"Dual Main"}}}"#
    }

    fn fixture_dir(name: &str) -> PathBuf {
        let suffix = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("xiii-clanlist-preview-{name}-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup_dir(dir: PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    fn write_required_message_files(dir: &Path) {
        fs::write(
            dir.join("main_roster_message_ids.json"),
            "[1498766315299799185]",
        )
        .unwrap();
        fs::write(
            dir.join("admin_roster_message_ids.json"),
            "[1498766321867821218]",
        )
        .unwrap();
        fs::write(
            dir.join("steam_roster_message_ids.json"),
            "[1500086435506683954]",
        )
        .unwrap();
    }

    fn env_for_dir(dir: &Path, token: &str, sheet: &str) -> String {
        format!(
            r#"
DISCORD_TOKEN={token}
DISCORD_CLIENT_ID=1501644078012694558
XIII_GUILD_ID=1498022112114249819
CLANLIST_ENABLED=false
LEGACY_CLANLIST_DATA_DIR={}
CLANLIST_MAIN_CHANNEL_ID=1498762828666896535
CLANLIST_ADMIN_CHANNEL_ID=1498763049102868672
CLANLIST_STEAM_CHANNEL_ID=1500081418506862754
CLANLIST_MAIN_ROLE_IDS=1498022112131289217,1498022112131289216
CLANLIST_ADMIN_ROLE_IDS=1498022112131289217,1498057076151422976
CLANLIST_STEAM_ACTIVE_ROLE_ID=1498022112114249827
CLANLIST_GOOGLE_SERVICE_ACCOUNT_FILE=credentials.json
CLANLIST_GOOGLE_SHEET_ID={sheet}
VACATION_ROLE_ID=1498022112131289214
VOICE_VACATION_MARKER_ROLE_ID=1498113605768314921
"#,
            dir.display()
        )
    }
}
