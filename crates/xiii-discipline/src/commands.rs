#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisciplinePermission {
    Allowed,
    Denied,
}

pub fn can_moderate(
    has_administrator: bool,
    has_manage_guild: bool,
    member_role_ids: &[u64],
    officer_role_ids: &[u64],
) -> DisciplinePermission {
    if has_administrator
        || has_manage_guild
        || member_role_ids
            .iter()
            .any(|role_id| officer_role_ids.contains(role_id))
    {
        DisciplinePermission::Allowed
    } else {
        DisciplinePermission::Denied
    }
}

pub fn valid_target(
    is_bot: bool,
    is_server_owner: bool,
    has_main_clan_role: bool,
) -> Result<(), String> {
    if is_bot {
        return Err("target must not be a bot".to_owned());
    }
    if is_server_owner {
        return Err("target must not be the server owner".to_owned());
    }
    if !has_main_clan_role {
        return Err("target must have the main clan role".to_owned());
    }
    Ok(())
}
