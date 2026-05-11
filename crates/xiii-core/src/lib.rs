use serde::{Deserialize, Serialize};
use std::fmt;

pub mod ids;
pub mod module;
pub mod report;
pub mod time;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleId {
    Core,
    Config,
    Db,
    Discord,
    Scheduler,
    Permissions,
    Tickets,
    VoiceActivity,
    Recruit,
    Vacation,
    Discipline,
    Clanlist,
    TempVoice,
}

impl ModuleId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Config => "config",
            Self::Db => "db",
            Self::Discord => "discord",
            Self::Scheduler => "scheduler",
            Self::Permissions => "permissions",
            Self::Tickets => "tickets",
            Self::VoiceActivity => "voice_activity",
            Self::Recruit => "recruit",
            Self::Vacation => "vacation",
            Self::Discipline => "discipline",
            Self::Clanlist => "clanlist",
            Self::TempVoice => "temp_voice",
        }
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateKind {
    Sqlite,
    JsonFile,
    JsonDirectory,
    DiscordMessage,
    ExternalService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    Disabled,
    ReadOnly,
    WriteAfterCutover,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDependency {
    pub path: String,
    pub kind: StateKind,
    pub access: AccessMode,
    pub notes: String,
}

impl StateDependency {
    pub fn sqlite(path: impl Into<String>, notes: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: StateKind::Sqlite,
            access: AccessMode::ReadOnly,
            notes: notes.into(),
        }
    }

    pub fn json_file(path: impl Into<String>, notes: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: StateKind::JsonFile,
            access: AccessMode::ReadOnly,
            notes: notes.into(),
        }
    }

    pub fn json_directory(path: impl Into<String>, notes: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: StateKind::JsonDirectory,
            access: AccessMode::ReadOnly,
            notes: notes.into(),
        }
    }

    pub fn write_after_cutover(mut self) -> Self {
        self.access = AccessMode::WriteAfterCutover;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvDependency {
    pub old_name: Option<String>,
    pub new_name: String,
    pub required: bool,
    pub secret: bool,
    pub purpose: String,
}

impl EnvDependency {
    pub fn new(
        old_name: Option<&str>,
        new_name: &str,
        required: bool,
        secret: bool,
        purpose: &str,
    ) -> Self {
        Self {
            old_name: old_name.map(str::to_owned),
            new_name: new_name.to_owned(),
            required,
            secret,
            purpose: purpose.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandDescriptor {
    pub name: String,
    pub subcommands_or_options: Vec<String>,
    pub legacy_source: String,
    pub mutates_production: bool,
}

impl SlashCommandDescriptor {
    pub fn new(name: impl Into<String>, legacy_source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            subcommands_or_options: Vec::new(),
            legacy_source: legacy_source.into(),
            mutates_production: false,
        }
    }

    pub fn with_options(mut self, options: &[&str]) -> Self {
        self.subcommands_or_options = options.iter().map(|item| (*item).to_owned()).collect();
        self
    }

    pub fn mutating(mut self) -> Self {
        self.mutates_production = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRoute {
    pub custom_id_pattern: String,
    pub legacy_source: String,
    pub persistent: bool,
    pub mutates_production: bool,
}

impl ComponentRoute {
    pub fn new(pattern: impl Into<String>, legacy_source: impl Into<String>) -> Self {
        Self {
            custom_id_pattern: pattern.into(),
            legacy_source: legacy_source.into(),
            persistent: true,
            mutates_production: false,
        }
    }

    pub fn transient(mut self) -> Self {
        self.persistent = false;
        self
    }

    pub fn mutating(mut self) -> Self {
        self.mutates_production = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerJobDescriptor {
    pub name: String,
    pub interval_seconds: Option<u64>,
    pub legacy_source: String,
    pub must_not_duplicate: bool,
    pub mutates_production: bool,
}

impl SchedulerJobDescriptor {
    pub fn interval(
        name: impl Into<String>,
        seconds: u64,
        legacy_source: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            interval_seconds: Some(seconds),
            legacy_source: legacy_source.into(),
            must_not_duplicate: true,
            mutates_production: false,
        }
    }

    pub fn startup(name: impl Into<String>, legacy_source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            interval_seconds: None,
            legacy_source: legacy_source.into(),
            must_not_duplicate: true,
            mutates_production: false,
        }
    }

    pub fn mutating(mut self) -> Self {
        self.mutates_production = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub id: ModuleId,
    pub name: String,
    pub old_source_path: String,
    pub migration_difficulty: String,
    pub state_dependencies: Vec<StateDependency>,
    pub env_dependencies: Vec<EnvDependency>,
    pub slash_commands: Vec<SlashCommandDescriptor>,
    pub component_routes: Vec<ComponentRoute>,
    pub scheduler_jobs: Vec<SchedulerJobDescriptor>,
    pub notes: Vec<String>,
}

impl ModuleManifest {
    pub fn new(
        id: ModuleId,
        name: impl Into<String>,
        old_source_path: impl Into<String>,
        migration_difficulty: impl Into<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            old_source_path: old_source_path.into(),
            migration_difficulty: migration_difficulty.into(),
            state_dependencies: Vec::new(),
            env_dependencies: Vec::new(),
            slash_commands: Vec::new(),
            component_routes: Vec::new(),
            scheduler_jobs: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_state(mut self, dependency: StateDependency) -> Self {
        self.state_dependencies.push(dependency);
        self
    }

    pub fn with_env(mut self, dependency: EnvDependency) -> Self {
        self.env_dependencies.push(dependency);
        self
    }

    pub fn with_command(mut self, command: SlashCommandDescriptor) -> Self {
        self.slash_commands.push(command);
        self
    }

    pub fn with_component(mut self, route: ComponentRoute) -> Self {
        self.component_routes.push(route);
        self
    }

    pub fn with_job(mut self, job: SchedulerJobDescriptor) -> Self {
        self.scheduler_jobs.push(job);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

pub trait Module {
    fn manifest(&self) -> ModuleManifest;
}

pub trait LegacyRepository {
    fn module_id(&self) -> ModuleId;
    fn state_dependencies(&self) -> &[StateDependency];
    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadOnly
    }
}

pub trait ComponentRouter {
    fn component_routes(&self) -> &[ComponentRoute];
}

pub trait SlashCommandRegistry {
    fn slash_commands(&self) -> &[SlashCommandDescriptor];
}

pub trait SchedulerJob {
    fn descriptor(&self) -> &SchedulerJobDescriptor;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Ok,
    Warn,
    Fail,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }

    pub fn is_failure(self) -> bool {
        matches!(self, Self::Fail)
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportItem {
    pub severity: Severity,
    pub scope: String,
    pub message: String,
}

impl ReportItem {
    pub fn new(severity: Severity, scope: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity,
            scope: scope.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub items: Vec<ReportItem>,
}

impl Report {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(
        &mut self,
        severity: Severity,
        scope: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.items.push(ReportItem::new(severity, scope, message));
    }

    pub fn ok(&mut self, scope: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Ok, scope, message);
    }

    pub fn warn(&mut self, scope: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Warn, scope, message);
    }

    pub fn fail(&mut self, scope: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Fail, scope, message);
    }

    pub fn extend(&mut self, other: Report) {
        self.items.extend(other.items);
    }

    pub fn has_failures(&self) -> bool {
        self.items.iter().any(|item| item.severity.is_failure())
    }

    pub fn counts(&self) -> SeverityCounts {
        let mut counts = SeverityCounts::default();
        for item in &self.items {
            match item.severity {
                Severity::Ok => counts.ok += 1,
                Severity::Warn => counts.warn += 1,
                Severity::Fail => counts.fail += 1,
            }
        }
        counts
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub ok: usize,
    pub warn: usize,
    pub fail: usize,
}

#[cfg(test)]
mod tests {
    use super::{Report, Severity};

    #[test]
    fn report_aggregates_severity_counts() {
        let mut report = Report::new();
        report.ok("config", "guild id parsed");
        report.warn("voice", "active sessions differ from audit");
        report.fail("tickets", "legacy DB missing");

        let counts = report.counts();
        assert_eq!(counts.ok, 1);
        assert_eq!(counts.warn, 1);
        assert_eq!(counts.fail, 1);
        assert!(report.has_failures());
        assert_eq!(report.items[2].severity, Severity::Fail);
    }
}
