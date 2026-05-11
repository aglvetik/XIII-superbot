use serde::{Deserialize, Serialize};
use xiii_core::ModuleId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub module_id: ModuleId,
    pub name: String,
    pub required_roles: Vec<u64>,
    pub required_channel: Option<u64>,
    pub notes: String,
}

impl PermissionRule {
    pub fn new(module_id: ModuleId, name: &str, notes: &str) -> Self {
        Self {
            module_id,
            name: name.to_owned(),
            required_roles: Vec::new(),
            required_channel: None,
            notes: notes.to_owned(),
        }
    }

    pub fn roles(mut self, roles: &[u64]) -> Self {
        self.required_roles = roles.to_vec();
        self
    }

    pub fn channel(mut self, channel_id: u64) -> Self {
        self.required_channel = Some(channel_id);
        self
    }
}

pub fn critical_rules() -> Vec<PermissionRule> {
    vec![
        PermissionRule::new(
            ModuleId::Tickets,
            "custom ticket roles",
            "Only custom command roles may create custom tickets.",
        )
        .roles(&[1_498_022_112_131_289_216, 1_498_022_112_131_289_217]),
        PermissionRule::new(
            ModuleId::Recruit,
            "decision channel",
            "Recruit commands are locked to the decision channel.",
        )
        .channel(1_500_136_438_791_147_651),
        PermissionRule::new(
            ModuleId::Discipline,
            "officer roles",
            "Discipline components require moderator/officer permission.",
        )
        .roles(&[1_498_022_112_131_289_216, 1_498_022_112_131_289_217]),
        PermissionRule::new(
            ModuleId::Vacation,
            "officer channel visibility",
            "Vacation approval uses officer-channel visibility plus admin/owner checks.",
        )
        .channel(1_500_438_001_514_184_714),
    ]
}
