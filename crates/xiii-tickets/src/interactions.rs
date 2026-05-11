pub const PANEL_APPLY: &str = "panel_apply";
pub const PANEL_QUESTION: &str = "panel_question";
pub const PANEL_IDEA: &str = "panel_idea";
pub const TICKET_CLOSE: &str = "ticket_close";
pub const TICKET_STAFF_NOTES: &str = "ticket_staff_notes";
pub const TICKET_CLOSE_CONFIRM: &str = "ticket_close_confirm";
pub const TICKET_CLOSE_CANCEL: &str = "ticket_close_cancel";
pub const TICKET_DELETE: &str = "ticket_delete";
pub const TICKET_REOPEN_MOD: &str = "ticket_reopen_mod";
pub const DM_REOPEN_GENERIC: &str = "dm_reopen_generic";
pub const NOTES_DELETE: &str = "notes_delete";
pub const APP_DECISION_ACCEPT: &str = "app_decision_accept";
pub const APP_DECISION_REJECT: &str = "app_decision_reject";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketComponentRoute {
    OpenApplication,
    OpenQuestion,
    OpenIdea,
    Close,
    StaffNotes,
    CloseConfirm,
    CloseCancel,
    Delete,
    ReopenMod,
    DmReopen,
    NotesDelete,
    ApplicationAccept,
    ApplicationReject,
}

pub fn route_ticket_panel(custom_id: &str) -> Option<&'static str> {
    match custom_id {
        PANEL_APPLY => Some("application"),
        PANEL_QUESTION => Some("complaint"),
        PANEL_IDEA => Some("idea"),
        _ => None,
    }
}

pub fn route_ticket_component(custom_id: &str) -> Option<TicketComponentRoute> {
    match custom_id {
        PANEL_APPLY => Some(TicketComponentRoute::OpenApplication),
        PANEL_QUESTION => Some(TicketComponentRoute::OpenQuestion),
        PANEL_IDEA => Some(TicketComponentRoute::OpenIdea),
        TICKET_CLOSE => Some(TicketComponentRoute::Close),
        TICKET_STAFF_NOTES => Some(TicketComponentRoute::StaffNotes),
        TICKET_CLOSE_CONFIRM => Some(TicketComponentRoute::CloseConfirm),
        TICKET_CLOSE_CANCEL => Some(TicketComponentRoute::CloseCancel),
        TICKET_DELETE => Some(TicketComponentRoute::Delete),
        TICKET_REOPEN_MOD => Some(TicketComponentRoute::ReopenMod),
        DM_REOPEN_GENERIC => Some(TicketComponentRoute::DmReopen),
        NOTES_DELETE => Some(TicketComponentRoute::NotesDelete),
        APP_DECISION_ACCEPT => Some(TicketComponentRoute::ApplicationAccept),
        APP_DECISION_REJECT => Some(TicketComponentRoute::ApplicationReject),
        _ => None,
    }
}
