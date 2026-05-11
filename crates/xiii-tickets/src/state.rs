use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketType {
    Application,
    Complaint,
    Idea,
    Custom,
}

impl TicketType {
    pub fn as_legacy_value(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Complaint => "other",
            Self::Idea => "idea",
            Self::Custom => "custom",
        }
    }

    pub fn counter_name(self) -> &'static str {
        match self {
            Self::Application => "xiii_legion:ticket_counter:application",
            Self::Complaint => "xiii_legion:ticket_counter:other",
            Self::Idea => "xiii_legion:ticket_counter:idea",
            Self::Custom => "xiii_legion:ticket_counter:custom",
        }
    }

    pub fn from_legacy_value(value: &str) -> Self {
        match value {
            "application" => Self::Application,
            "idea" => Self::Idea,
            "custom" => Self::Custom,
            _ => Self::Complaint,
        }
    }

    pub fn channel_prefix(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Complaint => "complaint",
            Self::Idea => "idea",
            Self::Custom => "ticket",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketStatus {
    Reserved,
    Open,
    Closed,
    Deleted,
}

impl TicketStatus {
    pub fn as_legacy_value(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Deleted => "deleted",
        }
    }

    pub fn from_legacy_value(value: &str) -> Self {
        match value {
            "reserved" => Self::Reserved,
            "closed" => Self::Closed,
            "deleted" => Self::Deleted,
            _ => Self::Open,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Reserved | Self::Open)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: i64,
    pub number: i64,
    pub ticket_type: TicketType,
    pub status: TicketStatus,
    pub channel_id: Option<u64>,
    pub user_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketRecord {
    pub ticket_id: i64,
    pub ticket_name: Option<String>,
    pub opener_id: u64,
    pub ticket_type: TicketType,
    pub channel_id: Option<u64>,
    pub status: TicketStatus,
    pub created_at_utc: String,
    pub closed_at_utc: Option<String>,
    pub reopen_until_utc: Option<String>,
}

impl TicketRecord {
    pub fn to_ticket(&self) -> Ticket {
        Ticket {
            id: self.ticket_id,
            number: parse_ticket_number(self.ticket_name.as_deref()).unwrap_or(self.ticket_id),
            ticket_type: self.ticket_type,
            status: self.status,
            channel_id: self.channel_id,
            user_id: self.opener_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketPanelState {
    pub source: String,
    pub guild_id: u64,
    pub bot_user_id: u64,
    pub channel_id: u64,
    pub panel_message_id: u64,
    pub created_at_utc: String,
    pub legacy_panel_message_id: Option<u64>,
}

impl TicketPanelState {
    pub fn fresh(guild_id: u64, bot_user_id: u64, channel_id: u64, panel_message_id: u64) -> Self {
        Self {
            source: "fresh_bootstrap".to_owned(),
            guild_id,
            bot_user_id,
            channel_id,
            panel_message_id,
            created_at_utc: utc_now_string(),
            legacy_panel_message_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservedTicket {
    pub ticket_id: i64,
    pub number: i64,
    pub ticket_name: String,
    pub ticket_type: TicketType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleFormRow {
    pub sheet_row: i64,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficerReviewDraft {
    pub sheet_row: i64,
    pub signature: String,
    pub target_ticket_channel_id: u64,
    pub ticket_number: Option<i64>,
    pub applicant_name: Option<String>,
}

pub fn utc_now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn format_utc(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ticket_number(name: Option<&str>) -> Option<i64> {
    let name = name?;
    name.rsplit('-').next()?.parse().ok()
}
