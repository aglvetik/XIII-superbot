pub fn can_use_custom_ticket_command(member_role_ids: &[u64], allowed_role_ids: &[u64]) -> bool {
    member_role_ids
        .iter()
        .any(|role_id| allowed_role_ids.contains(role_id))
}

pub fn can_moderate_tickets(member_role_ids: &[u64], moderator_role_ids: &[u64]) -> bool {
    member_role_ids
        .iter()
        .any(|role_id| moderator_role_ids.contains(role_id))
}

pub fn can_accept_application(member_role_ids: &[u64], accept_role_ids: &[u64]) -> bool {
    member_role_ids
        .iter()
        .any(|role_id| accept_role_ids.contains(role_id))
}

pub fn is_accept_prefix(content: &str) -> bool {
    matches!(content.trim(), "!accept" | "!принять")
}

pub fn is_reject_prefix(content: &str) -> bool {
    matches!(content.trim(), "!reject" | "!отклонить")
}

pub fn is_panel_prefix(content: &str) -> bool {
    content.trim() == "!panel"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketTextCommand {
    Panel,
    Accept,
    Reject,
}

pub fn route_text_command(content: &str) -> Option<TicketTextCommand> {
    if is_panel_prefix(content) {
        Some(TicketTextCommand::Panel)
    } else if is_accept_prefix(content) {
        Some(TicketTextCommand::Accept)
    } else if is_reject_prefix(content) {
        Some(TicketTextCommand::Reject)
    } else {
        None
    }
}
