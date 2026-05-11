use crate::state::{EscalationOutcome, Punishment, PunishmentStatus, PunishmentType};

pub fn escalation_for_new_punishment(
    existing: &[Punishment],
    requested: PunishmentType,
    user_id: u64,
) -> EscalationOutcome {
    let active_count = |kind| {
        existing
            .iter()
            .filter(|punishment| {
                punishment.user_id == user_id
                    && punishment.kind == kind
                    && punishment.status == PunishmentStatus::Active
            })
            .count()
    };

    match requested {
        PunishmentType::Warning if active_count(PunishmentType::Warning) >= 1 => {
            EscalationOutcome::Issue(PunishmentType::Verbal)
        }
        PunishmentType::Verbal if active_count(PunishmentType::Verbal) >= 1 => {
            EscalationOutcome::Issue(PunishmentType::Strict)
        }
        PunishmentType::Strict if active_count(PunishmentType::Strict) >= 1 => {
            EscalationOutcome::ClanRemoval
        }
        other => EscalationOutcome::Issue(other),
    }
}

pub fn expires_after_days(
    kind: PunishmentType,
    warning_days: u64,
    verbal_days: u64,
) -> Option<u64> {
    match kind {
        PunishmentType::Warning => Some(warning_days),
        PunishmentType::Verbal => Some(verbal_days),
        PunishmentType::Strict => None,
    }
}
