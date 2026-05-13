use crate::interactions::{PANEL_APPLY, PANEL_IDEA, PANEL_QUESTION, TICKET_CLOSE};
use crate::render::{
    panel_description, panel_title, RenderField, TicketEmbedDraft, TranscriptMessage,
    LEGACY_PANEL_COLOR,
};
use crate::runtime::TicketCreationPlan;
use crate::state::TicketType;
use std::sync::Arc;
use twilight_http::request::AuditLogReason;
use twilight_http::Client as DiscordHttpClient;
use twilight_model::channel::message::component::{ActionRow, Button, ButtonStyle, Component};
use twilight_model::channel::message::embed::{Embed, EmbedField, EmbedFooter};
use twilight_model::channel::message::{AllowedMentions, Message as DiscordMessage};
use twilight_model::channel::permission_overwrite::{
    PermissionOverwrite as ChannelPermissionOverwrite,
    PermissionOverwriteType as ChannelPermissionOverwriteType,
};
use twilight_model::channel::{Channel, ChannelType};
use twilight_model::guild::Permissions;
use twilight_model::http::attachment::Attachment;
use twilight_model::http::permission_overwrite::{
    PermissionOverwrite as HttpPermissionOverwrite,
    PermissionOverwriteType as HttpPermissionOverwriteType,
};
use twilight_model::id::marker::{
    ChannelMarker, GenericMarker, GuildMarker, MessageMarker, RoleMarker, UserMarker,
};
use twilight_model::id::Id;

const TICKET_CHANNEL_CREATE_REASON: &str = "XIII ticket channel create";
const TICKET_CHANNEL_UPDATE_REASON: &str = "XIII ticket channel update";
const TICKET_CHANNEL_DELETE_REASON: &str = "XIII ticket channel delete";
const TICKET_PERMISSION_UPDATE_REASON: &str = "XIII ticket permission update";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketChannelCreateRequest {
    pub guild_id: u64,
    pub category_id: u64,
    pub channel_name: String,
    pub opener_user_id: u64,
    pub support_role_id: u64,
    pub moderator_role_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketPermissionPlan {
    pub user_id: u64,
    pub support_role_id: u64,
    pub moderator_role_ids: Vec<u64>,
    pub deny_everyone_read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketMessagePayload {
    pub content: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: u32,
    pub allowed_role_mentions: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketTranscriptPayload {
    pub transcript_channel_id: u64,
    pub filename: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficerReviewPayload {
    pub officer_review_channel_id: u64,
    pub title: Option<String>,
    pub description: String,
    pub color: u32,
    pub allowed_role_mentions: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketDmPayload {
    pub user_id: u64,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketButtonSpec {
    pub custom_id: &'static str,
    pub label: &'static str,
    pub style: ButtonStyle,
}

#[derive(Clone)]
pub struct TicketDiscordHttp {
    client: Arc<DiscordHttpClient>,
}

impl TicketDiscordHttp {
    pub fn new(client: Arc<DiscordHttpClient>) -> Self {
        Self { client }
    }

    pub async fn create_ticket_channel(
        &self,
        request: &TicketChannelCreateRequest,
    ) -> Result<Channel, String> {
        let overwrites = permission_overwrites_for_ticket(
            request.guild_id,
            request.opener_user_id,
            request.support_role_id,
            &request.moderator_role_ids,
            &[],
        );
        self.client
            .create_guild_channel(
                Id::<GuildMarker>::new(request.guild_id),
                &request.channel_name,
            )
            .kind(ChannelType::GuildText)
            .parent_id(Id::<ChannelMarker>::new(request.category_id))
            .permission_overwrites(&overwrites)
            .reason(TICKET_CHANNEL_CREATE_REASON)
            .await
            .map_err(|err| {
                format!(
                    "failed to create ticket channel {}: {err}",
                    request.channel_name
                )
            })?
            .model()
            .await
            .map_err(|err| format!("failed to decode created ticket channel: {err}"))
    }

    pub async fn send_ticket_panel(&self, channel_id: u64) -> Result<DiscordMessage, String> {
        let embed = simple_embed(
            Some(panel_title()),
            Some(panel_description()),
            LEGACY_PANEL_COLOR,
        )
        .ok_or_else(|| "ticket panel embed could not be constructed".to_owned())?;
        let components = panel_components();
        self.client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&AllowedMentions::default()))
            .embeds(&[embed])
            .components(&components)
            .await
            .map_err(|err| format!("failed to create ticket panel: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket panel message: {err}"))
    }

    pub async fn edit_ticket_panel(
        &self,
        channel_id: u64,
        message_id: u64,
    ) -> Result<DiscordMessage, String> {
        let embed = simple_embed(
            Some(panel_title()),
            Some(panel_description()),
            LEGACY_PANEL_COLOR,
        )
        .ok_or_else(|| "ticket panel embed could not be constructed".to_owned())?;
        let components = panel_components();
        self.client
            .update_message(
                Id::<ChannelMarker>::new(channel_id),
                Id::<MessageMarker>::new(message_id),
            )
            .allowed_mentions(Some(&AllowedMentions::default()))
            .content(None)
            .embeds(Some(&[embed]))
            .components(Some(&components))
            .await
            .map_err(|err| format!("failed to edit ticket panel {message_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket panel edit: {err}"))
    }

    pub async fn send_ticket_open_message(
        &self,
        channel_id: u64,
        payload: &TicketMessagePayload,
    ) -> Result<DiscordMessage, String> {
        let allowed = allowed_mentions_for_roles(&payload.allowed_role_mentions);
        let embed = simple_embed(
            payload.title.as_deref(),
            payload.description.as_deref(),
            payload.color,
        );
        let components = vec![action_row(vec![button(
            TICKET_CLOSE,
            crate::render::TICKET_CLOSE_LABEL,
            ButtonStyle::Danger,
        )])];
        let mut request = self
            .client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&allowed))
            .components(&components);
        let embeds;
        if let Some(content) = payload.content.as_deref() {
            request = request.content(content);
        }
        if let Some(embed) = embed {
            embeds = [embed];
            request = request.embeds(&embeds);
        }
        request
            .await
            .map_err(|err| format!("failed to send ticket opening message: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket opening message: {err}"))
    }

    pub async fn send_channel_message(
        &self,
        channel_id: u64,
        content: &str,
        allowed_role_mentions: &[u64],
    ) -> Result<DiscordMessage, String> {
        let allowed = allowed_mentions_for_roles(allowed_role_mentions);
        self.client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&allowed))
            .content(content)
            .await
            .map_err(|err| format!("failed to send ticket channel message: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket channel message: {err}"))
    }

    pub async fn send_channel_embed_message(
        &self,
        channel_id: u64,
        embed: &Embed,
        components: Option<Vec<Component>>,
    ) -> Result<DiscordMessage, String> {
        let embeds = [embed.clone()];
        let allowed = AllowedMentions::default();
        let mut request = self
            .client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&allowed))
            .embeds(&embeds);
        if let Some(components) = components.as_ref() {
            request = request.components(components);
        }
        request
            .await
            .map_err(|err| format!("failed to send ticket embed message: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket embed message: {err}"))
    }

    pub async fn send_officer_review(
        &self,
        payload: &OfficerReviewPayload,
    ) -> Result<DiscordMessage, String> {
        let allowed = allowed_mentions_for_roles(&payload.allowed_role_mentions);
        let embed = simple_embed(
            payload.title.as_deref(),
            Some(&payload.description),
            payload.color,
        )
        .ok_or_else(|| "ticket officer review embed could not be constructed".to_owned())?;
        self.client
            .create_message(Id::<ChannelMarker>::new(payload.officer_review_channel_id))
            .allowed_mentions(Some(&allowed))
            .embeds(&[embed])
            .await
            .map_err(|err| format!("failed to send ticket officer review: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket officer review: {err}"))
    }

    pub async fn dm_user(&self, payload: &TicketDmPayload) -> Result<Option<u64>, String> {
        let channel_id = self.open_dm_channel(payload.user_id).await?;
        let message = self
            .client
            .create_message(channel_id)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .content(&payload.content)
            .await
            .map_err(|err| format!("failed to send ticket DM to {}: {err}", payload.user_id))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket DM message: {err}"))?;
        Ok(Some(message.id.get()))
    }

    pub async fn send_dm_embed_message(
        &self,
        user_id: u64,
        embed: &Embed,
        components: Option<Vec<Component>>,
    ) -> Result<Option<u64>, String> {
        let channel_id = self.open_dm_channel(user_id).await?;
        let embeds = [embed.clone()];
        let allowed = AllowedMentions::default();
        let mut request = self
            .client
            .create_message(channel_id)
            .allowed_mentions(Some(&allowed))
            .embeds(&embeds);
        if let Some(components) = components.as_ref() {
            request = request.components(components);
        }
        let message = request
            .await
            .map_err(|err| format!("failed to send ticket DM embed to {user_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket DM embed message: {err}"))?;
        Ok(Some(message.id.get()))
    }

    pub async fn send_dm_transcript(
        &self,
        user_id: u64,
        payload: &TicketTranscriptPayload,
    ) -> Result<Option<u64>, String> {
        let channel_id = self.open_dm_channel(user_id).await?;
        let attachment = Attachment::from_bytes(
            payload.filename.clone(),
            payload.body.clone().into_bytes(),
            0,
        );
        let attachments = [attachment];
        let message = self
            .client
            .create_message(channel_id)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .attachments(&attachments)
            .await
            .map_err(|err| format!("failed to send ticket DM transcript to {user_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket DM transcript message: {err}"))?;
        Ok(Some(message.id.get()))
    }

    pub async fn rename_channel(&self, channel_id: u64, new_name: &str) -> Result<Channel, String> {
        self.client
            .update_channel(Id::<ChannelMarker>::new(channel_id))
            .name(new_name)
            .reason(TICKET_CHANNEL_UPDATE_REASON)
            .await
            .map_err(|err| format!("failed to rename ticket channel {channel_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode renamed ticket channel: {err}"))
    }

    pub async fn set_channel_permissions(
        &self,
        channel_id: u64,
        overwrite: &ChannelPermissionOverwrite,
    ) -> Result<(), String> {
        let overwrite = http_permission_overwrite(overwrite);
        self.client
            .update_channel_permission(Id::<ChannelMarker>::new(channel_id), &overwrite)
            .reason(TICKET_PERMISSION_UPDATE_REASON)
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to update ticket channel permission: {err}"))
    }

    pub async fn delete_member_channel_permission(
        &self,
        channel_id: u64,
        user_id: u64,
    ) -> Result<(), String> {
        self.client
            .delete_channel_permission(Id::<ChannelMarker>::new(channel_id))
            .member(Id::<UserMarker>::new(user_id))
            .reason(TICKET_PERMISSION_UPDATE_REASON)
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to delete ticket member permission: {err}"))
    }

    pub async fn delete_channel(&self, channel_id: u64) -> Result<(), String> {
        self.client
            .delete_channel(Id::<ChannelMarker>::new(channel_id))
            .reason(TICKET_CHANNEL_DELETE_REASON)
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to delete ticket channel {channel_id}: {err}"))
    }

    pub async fn delete_message(&self, channel_id: u64, message_id: u64) -> Result<(), String> {
        self.client
            .delete_message(
                Id::<ChannelMarker>::new(channel_id),
                Id::<MessageMarker>::new(message_id),
            )
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to delete ticket message {message_id}: {err}"))
    }

    pub async fn send_transcript(
        &self,
        payload: &TicketTranscriptPayload,
    ) -> Result<DiscordMessage, String> {
        let attachment = Attachment::from_bytes(
            payload.filename.clone(),
            payload.body.clone().into_bytes(),
            0,
        );
        let attachments = [attachment];
        self.client
            .create_message(Id::<ChannelMarker>::new(payload.transcript_channel_id))
            .allowed_mentions(Some(&AllowedMentions::default()))
            .attachments(&attachments)
            .await
            .map_err(|err| format!("failed to send ticket transcript: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode ticket transcript message: {err}"))
    }

    pub async fn fetch_transcript_messages(
        &self,
        channel_id: u64,
        max_messages: usize,
    ) -> Result<Vec<TranscriptMessage>, String> {
        let mut output = Vec::new();
        let mut before: Option<Id<MessageMarker>> = None;
        while output.len() < max_messages {
            let limit = (max_messages - output.len()).min(100) as u16;
            let request = self
                .client
                .channel_messages(Id::<ChannelMarker>::new(channel_id));
            let response = match before {
                Some(message_id) => request.before(message_id).limit(limit).await,
                None => request.limit(limit).await,
            }
            .map_err(|err| format!("failed to fetch ticket transcript messages: {err}"))?;
            let mut messages = response
                .model()
                .await
                .map_err(|err| format!("failed to decode ticket transcript messages: {err}"))?;
            if messages.is_empty() {
                break;
            }
            before = messages.last().map(|message| message.id);
            for message in messages.drain(..) {
                output.push(TranscriptMessage {
                    author_id: message.author.id.get(),
                    author_name: message
                        .author
                        .global_name
                        .clone()
                        .unwrap_or_else(|| message.author.name.clone()),
                    timestamp_utc: message.timestamp.iso_8601().to_string(),
                    content: message.content,
                    attachment_urls: message
                        .attachments
                        .into_iter()
                        .map(|attachment| attachment.url)
                        .collect(),
                });
            }
            if output.len() >= max_messages || limit < 100 {
                break;
            }
        }
        output.reverse();
        Ok(output)
    }

    pub async fn add_member_role(
        &self,
        guild_id: u64,
        user_id: u64,
        role_id: u64,
    ) -> Result<(), String> {
        self.client
            .add_guild_member_role(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(user_id),
                Id::<RoleMarker>::new(role_id),
            )
            .reason("XIII ticket accept")
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to add ticket accept role {role_id}: {err}"))
    }

    pub async fn remove_member_role(
        &self,
        guild_id: u64,
        user_id: u64,
        role_id: u64,
    ) -> Result<(), String> {
        self.client
            .remove_guild_member_role(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(user_id),
                Id::<RoleMarker>::new(role_id),
            )
            .reason("XIII ticket accept")
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to remove ticket role {role_id}: {err}"))
    }

    pub async fn update_member_nick(
        &self,
        guild_id: u64,
        user_id: u64,
        nickname: &str,
    ) -> Result<(), String> {
        self.client
            .update_guild_member(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(user_id),
            )
            .nick(Some(nickname))
            .reason("XIII ticket accept nickname")
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to update ticket accept nickname: {err}"))
    }
}

impl TicketChannelCreateRequest {
    pub fn from_plan(
        guild_id: u64,
        category_id: u64,
        support_role_id: u64,
        moderator_role_ids: Vec<u64>,
        plan: &TicketCreationPlan,
    ) -> Self {
        Self {
            guild_id,
            category_id,
            channel_name: plan.channel_name.clone(),
            opener_user_id: plan.opener_user_id,
            support_role_id,
            moderator_role_ids,
        }
    }

    pub fn permission_plan(&self) -> TicketPermissionPlan {
        TicketPermissionPlan {
            user_id: self.opener_user_id,
            support_role_id: self.support_role_id,
            moderator_role_ids: self.moderator_role_ids.clone(),
            deny_everyone_read: true,
        }
    }
}

pub fn permission_overwrites_for_ticket(
    guild_id: u64,
    opener_user_id: u64,
    support_role_id: u64,
    moderator_role_ids: &[u64],
    extra_member_ids: &[u64],
) -> Vec<ChannelPermissionOverwrite> {
    let mut overwrites = vec![
        role_overwrite(
            guild_id,
            Permissions::empty(),
            ticket_read_write_permissions(),
        ),
        member_overwrite(
            opener_user_id,
            ticket_read_write_permissions(),
            Permissions::empty(),
        ),
        role_overwrite(
            support_role_id,
            ticket_read_write_permissions(),
            Permissions::empty(),
        ),
    ];
    for role_id in moderator_role_ids {
        if *role_id != support_role_id {
            overwrites.push(role_overwrite(
                *role_id,
                ticket_read_write_permissions(),
                Permissions::empty(),
            ));
        }
    }
    for user_id in extra_member_ids {
        if *user_id != opener_user_id {
            overwrites.push(member_overwrite(
                *user_id,
                ticket_read_write_permissions(),
                Permissions::empty(),
            ));
        }
    }
    overwrites
}

pub fn closed_ticket_owner_overwrite(user_id: u64) -> ChannelPermissionOverwrite {
    member_overwrite(
        user_id,
        Permissions::empty(),
        Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY,
    )
}

pub fn reopen_ticket_owner_overwrite(user_id: u64) -> ChannelPermissionOverwrite {
    member_overwrite(
        user_id,
        ticket_read_write_permissions(),
        Permissions::empty(),
    )
}

pub fn ticket_open_payload(
    plan: &TicketCreationPlan,
    ticket_type: TicketType,
) -> TicketMessagePayload {
    let opener_mention = format!("<@{}>", plan.opener_user_id);
    let ping_mention = plan
        .ping_role_id
        .map(|role_id| format!("<@&{role_id}>"))
        .unwrap_or_else(|| "Офицеры".to_owned());
    match ticket_type {
        TicketType::Application => TicketMessagePayload {
            content: Some(crate::render::application_form_message(&opener_mention)),
            title: None,
            description: None,
            color: LEGACY_PANEL_COLOR,
            allowed_role_mentions: Vec::new(),
        },
        TicketType::Complaint => TicketMessagePayload {
            content: Some(crate::render::complaint_main_message(
                &opener_mention,
                &ping_mention,
            )),
            title: Some(crate::render::TICKET_CREATED_TITLE.to_owned()),
            description: Some(crate::render::complaint_embed_description().to_owned()),
            color: crate::render::LEGACY_COMPLAINT_COLOR,
            allowed_role_mentions: plan.ping_role_id.into_iter().collect(),
        },
        TicketType::Idea => TicketMessagePayload {
            content: Some(crate::render::promotion_request_message(
                &opener_mention,
                &ping_mention,
            )),
            title: None,
            description: None,
            color: crate::render::LEGACY_PROMOTION_COLOR,
            allowed_role_mentions: plan.ping_role_id.into_iter().collect(),
        },
        TicketType::Custom => TicketMessagePayload {
            content: None,
            title: Some(crate::render::TICKET_CREATED_TITLE.to_owned()),
            description: Some(crate::render::custom_ticket_description().to_owned()),
            color: LEGACY_PANEL_COLOR,
            allowed_role_mentions: Vec::new(),
        },
    }
}

pub fn close_confirmation_payload() -> TicketMessagePayload {
    TicketMessagePayload {
        content: None,
        title: Some(crate::render::CLOSE_CONFIRM_TITLE.to_owned()),
        description: Some(crate::render::CLOSE_CONFIRM_DESCRIPTION.to_owned()),
        color: crate::render::LEGACY_COMPLAINT_COLOR,
        allowed_role_mentions: Vec::new(),
    }
}

pub fn transcript_payload(
    transcript_channel_id: u64,
    ticket_name: &str,
    body: String,
) -> TicketTranscriptPayload {
    TicketTranscriptPayload {
        transcript_channel_id,
        filename: format!("transcript-{ticket_name}.html"),
        body,
    }
}

pub fn officer_review_payload(
    officer_review_channel_id: u64,
    _ticket_name: &str,
    description: String,
    allowed_role_mentions: Vec<u64>,
) -> OfficerReviewPayload {
    OfficerReviewPayload {
        officer_review_channel_id,
        title: None,
        description,
        color: crate::render::LEGACY_OFFICER_REVIEW_COLOR,
        allowed_role_mentions,
    }
}

pub fn dm_payload(user_id: u64, content: impl Into<String>) -> TicketDmPayload {
    TicketDmPayload {
        user_id,
        content: content.into(),
    }
}

pub fn embed_from_draft(draft: &TicketEmbedDraft) -> Embed {
    Embed {
        author: None,
        color: Some(draft.color),
        description: draft.description.clone(),
        fields: draft
            .fields
            .iter()
            .map(embed_field_from_draft)
            .collect::<Vec<_>>(),
        footer: draft.footer.as_ref().map(|text| EmbedFooter {
            icon_url: None,
            proxy_icon_url: None,
            text: text.clone(),
        }),
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: Some(draft.title.clone()),
        url: None,
        video: None,
    }
}

pub fn close_confirmation_components() -> Vec<Component> {
    vec![action_row(vec![
        button(
            crate::interactions::TICKET_CLOSE_CONFIRM,
            crate::render::TICKET_CLOSE_CONFIRM_LABEL,
            ButtonStyle::Danger,
        ),
        button(
            crate::interactions::TICKET_CLOSE_CANCEL,
            crate::render::TICKET_CLOSE_CANCEL_LABEL,
            ButtonStyle::Secondary,
        ),
    ])]
}

pub fn after_close_components() -> Vec<Component> {
    vec![action_row(vec![
        button(
            crate::interactions::TICKET_DELETE,
            crate::render::TICKET_DELETE_LABEL,
            ButtonStyle::Danger,
        ),
        button(
            crate::interactions::TICKET_REOPEN_MOD,
            crate::render::TICKET_REOPEN_LABEL,
            ButtonStyle::Success,
        ),
    ])]
}

pub fn dm_reopen_components() -> Vec<Component> {
    vec![action_row(vec![button(
        crate::interactions::DM_REOPEN_GENERIC,
        crate::render::TICKET_REOPEN_LABEL,
        ButtonStyle::Success,
    )])]
}

pub fn allowed_mentions_are_limited(payload_roles: &[u64], configured_roles: &[u64]) -> bool {
    payload_roles
        .iter()
        .all(|role_id| configured_roles.contains(role_id))
}

pub fn allowed_mentions_for_roles(role_ids: &[u64]) -> AllowedMentions {
    AllowedMentions {
        roles: role_ids
            .iter()
            .copied()
            .map(Id::<RoleMarker>::new)
            .collect(),
        ..AllowedMentions::default()
    }
}

fn ticket_read_write_permissions() -> Permissions {
    Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY
}

fn role_overwrite(
    role_id: u64,
    allow: Permissions,
    deny: Permissions,
) -> ChannelPermissionOverwrite {
    ChannelPermissionOverwrite {
        allow,
        deny,
        id: Id::<GenericMarker>::new(role_id),
        kind: ChannelPermissionOverwriteType::Role,
    }
}

fn member_overwrite(
    user_id: u64,
    allow: Permissions,
    deny: Permissions,
) -> ChannelPermissionOverwrite {
    ChannelPermissionOverwrite {
        allow,
        deny,
        id: Id::<GenericMarker>::new(user_id),
        kind: ChannelPermissionOverwriteType::Member,
    }
}

fn http_permission_overwrite(overwrite: &ChannelPermissionOverwrite) -> HttpPermissionOverwrite {
    HttpPermissionOverwrite {
        allow: Some(overwrite.allow),
        deny: Some(overwrite.deny),
        id: overwrite.id,
        kind: match overwrite.kind {
            ChannelPermissionOverwriteType::Member => HttpPermissionOverwriteType::Member,
            ChannelPermissionOverwriteType::Role => HttpPermissionOverwriteType::Role,
            _ => HttpPermissionOverwriteType::Member,
        },
    }
}

fn embed_field_from_draft(field: &RenderField) -> EmbedField {
    EmbedField {
        inline: field.inline,
        name: field.name.clone(),
        value: field.value.clone(),
    }
}

fn simple_embed(title: Option<&str>, description: Option<&str>, color: u32) -> Option<Embed> {
    if title.is_none() && description.is_none() {
        return None;
    }
    Some(Embed {
        author: None,
        color: Some(color),
        description: description.map(str::to_owned),
        fields: Vec::new(),
        footer: None,
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: title.map(str::to_owned),
        url: None,
        video: None,
    })
}

impl TicketDiscordHttp {
    async fn open_dm_channel(&self, user_id: u64) -> Result<Id<ChannelMarker>, String> {
        self.client
            .create_private_channel(Id::<UserMarker>::new(user_id))
            .await
            .map_err(|err| format!("failed to open ticket DM for {user_id}: {err}"))?
            .model()
            .await
            .map(|channel| channel.id)
            .map_err(|err| format!("failed to decode ticket DM channel: {err}"))
    }
}

fn panel_components() -> Vec<Component> {
    vec![action_row(vec![
        button(
            ticket_panel_button_specs()[0].custom_id,
            ticket_panel_button_specs()[0].label,
            ticket_panel_button_specs()[0].style,
        ),
        button(
            ticket_panel_button_specs()[1].custom_id,
            ticket_panel_button_specs()[1].label,
            ticket_panel_button_specs()[1].style,
        ),
        button(
            ticket_panel_button_specs()[2].custom_id,
            ticket_panel_button_specs()[2].label,
            ticket_panel_button_specs()[2].style,
        ),
    ])]
}

pub fn ticket_panel_button_specs() -> [TicketButtonSpec; 3] {
    [
        TicketButtonSpec {
            custom_id: PANEL_APPLY,
            label: "📩 Подать заявку на вступление",
            style: ButtonStyle::Success,
        },
        TicketButtonSpec {
            custom_id: PANEL_QUESTION,
            label: "🚨 Подать жалобу",
            style: ButtonStyle::Primary,
        },
        TicketButtonSpec {
            custom_id: PANEL_IDEA,
            label: "Заявка на повышение",
            style: ButtonStyle::Secondary,
        },
    ]
}

fn action_row(buttons: Vec<Button>) -> Component {
    Component::ActionRow(ActionRow {
        components: buttons.into_iter().map(Component::Button).collect(),
    })
}

fn button(custom_id: &str, label: &str, style: ButtonStyle) -> Button {
    Button {
        custom_id: Some(custom_id.to_owned()),
        disabled: false,
        emoji: None,
        label: Some(label.to_owned()),
        style,
        url: None,
        sku_id: None,
    }
}
