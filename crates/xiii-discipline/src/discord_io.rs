use std::sync::Arc;
use twilight_http::request::AuditLogReason;
use twilight_http::Client as DiscordHttpClient;
use twilight_model::channel::message::component::Component;
use twilight_model::channel::message::embed::Embed;
use twilight_model::channel::message::{AllowedMentions, Message as DiscordMessage, MessageFlags};
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
use twilight_model::id::marker::{
    ApplicationMarker, ChannelMarker, GuildMarker, InteractionMarker, MessageMarker, RoleMarker,
    UserMarker,
};
use twilight_model::id::Id;
use twilight_model::util::Timestamp;

const DISCIPLINE_ROLE_AUDIT_REASON: &str = "XIII discipline role update";
const DISCIPLINE_TIMEOUT_REASON: &str = "XIII discipline timeout";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutRequest {
    pub user_id: u64,
    pub timeout_minutes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClanRemovalRequest {
    pub user_id: u64,
    pub remove_role_ids: Vec<u64>,
    pub add_guest_role_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplineDmDraft {
    pub user_id: u64,
    pub title: String,
    pub body: String,
}

pub fn clan_removal_request(
    user_id: u64,
    current_role_ids: &[u64],
    preserved_role_ids: &[u64],
    guest_role_id: u64,
) -> ClanRemovalRequest {
    let remove_role_ids = current_role_ids
        .iter()
        .copied()
        .filter(|role_id| !preserved_role_ids.contains(role_id))
        .collect();

    ClanRemovalRequest {
        user_id,
        remove_role_ids,
        add_guest_role_id: guest_role_id,
    }
}

pub fn punishment_dm(user_id: u64, title: &str, body: &str) -> DisciplineDmDraft {
    DisciplineDmDraft {
        user_id,
        title: title.to_owned(),
        body: body.to_owned(),
    }
}

#[derive(Clone)]
pub struct DisciplineDiscordHttp {
    client: Arc<DiscordHttpClient>,
}

impl DisciplineDiscordHttp {
    pub fn new(client: Arc<DiscordHttpClient>) -> Self {
        Self { client }
    }

    pub async fn edit_board(
        &self,
        channel_id: u64,
        message_id: u64,
        embeds: &[Embed],
        components: &[Component],
    ) -> Result<DiscordMessage, String> {
        self.client
            .update_message(
                Id::<ChannelMarker>::new(channel_id),
                Id::<MessageMarker>::new(message_id),
            )
            .allowed_mentions(Some(&AllowedMentions::default()))
            .content(None)
            .embeds(Some(embeds))
            .components(Some(components))
            .await
            .map_err(|err| format!("failed to edit discipline board: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode discipline board edit: {err}"))
    }

    pub async fn send_admin_log(
        &self,
        channel_id: u64,
        content: &str,
        embed: Option<Embed>,
    ) -> Result<DiscordMessage, String> {
        let allowed_mentions = AllowedMentions::default();
        let mut request = self
            .client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&allowed_mentions))
            .content(content);
        let embeds;
        if let Some(embed) = embed {
            embeds = vec![embed];
            request = request.embeds(&embeds);
        }
        request
            .await
            .map_err(|err| format!("failed to send discipline admin log: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode discipline admin log message: {err}"))
    }

    pub async fn dm_user(&self, draft: &DisciplineDmDraft) -> Result<Option<u64>, String> {
        let channel = self
            .client
            .create_private_channel(Id::<UserMarker>::new(draft.user_id))
            .await
            .map_err(|err| format!("failed to open discipline DM for {}: {err}", draft.user_id))?
            .model()
            .await
            .map_err(|err| format!("failed to decode discipline DM channel: {err}"))?;
        let content = format!("**{}**\n{}", draft.title, draft.body);
        let message = self
            .client
            .create_message(channel.id)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .content(&content)
            .await
            .map_err(|err| format!("failed to send discipline DM to {}: {err}", draft.user_id))?
            .model()
            .await
            .map_err(|err| format!("failed to decode discipline DM message: {err}"))?;
        Ok(Some(message.id.get()))
    }

    pub async fn apply_timeout(
        &self,
        guild_id: u64,
        request: &TimeoutRequest,
        now_unix: i64,
    ) -> Result<(), String> {
        let until = Timestamp::from_secs(now_unix + (request.timeout_minutes as i64 * 60))
            .map_err(|err| format!("failed to build Discord timeout timestamp: {err}"))?;
        self.client
            .update_guild_member(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(request.user_id),
            )
            .communication_disabled_until(Some(until))
            .reason(DISCIPLINE_TIMEOUT_REASON)
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to timeout user {}: {err}", request.user_id))?;
        Ok(())
    }

    pub async fn execute_clan_removal(
        &self,
        guild_id: u64,
        request: &ClanRemovalRequest,
    ) -> Result<(), String> {
        for role_id in &request.remove_role_ids {
            self.client
                .remove_guild_member_role(
                    Id::<GuildMarker>::new(guild_id),
                    Id::<UserMarker>::new(request.user_id),
                    Id::<RoleMarker>::new(*role_id),
                )
                .reason(DISCIPLINE_ROLE_AUDIT_REASON)
                .await
                .map(|_| ())
                .map_err(|err| {
                    format!(
                        "failed to remove role {role_id} during clan removal for {}: {err}",
                        request.user_id
                    )
                })?;
        }
        self.client
            .add_guild_member_role(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(request.user_id),
                Id::<RoleMarker>::new(request.add_guest_role_id),
            )
            .reason(DISCIPLINE_ROLE_AUDIT_REASON)
            .await
            .map(|_| ())
            .map_err(|err| {
                format!(
                    "failed to add guest role {} during clan removal for {}: {err}",
                    request.add_guest_role_id, request.user_id
                )
            })?;
        Ok(())
    }

    pub async fn respond_ephemeral(
        &self,
        application_id: u64,
        interaction_id: u64,
        token: &str,
        content: &str,
    ) -> Result<(), String> {
        let response = InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(InteractionResponseData {
                allowed_mentions: Some(AllowedMentions::default()),
                content: Some(content.to_owned()),
                flags: Some(MessageFlags::EPHEMERAL),
                ..InteractionResponseData::default()
            }),
        };
        self.client
            .interaction(Id::<ApplicationMarker>::new(application_id))
            .create_response(
                Id::<InteractionMarker>::new(interaction_id),
                token,
                &response,
            )
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to respond to discipline interaction: {err}"))
    }
}
