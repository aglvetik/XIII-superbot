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
    ApplicationMarker, ChannelMarker, GuildMarker, InteractionMarker, RoleMarker, UserMarker,
};
use twilight_model::id::Id;

const RECRUIT_ROLE_AUDIT_REASON: &str = "XIII recruit probation decision";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitRoleTransition {
    pub user_id: u64,
    pub remove_role_ids: Vec<u64>,
    pub add_role_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitDmDraft {
    pub user_id: u64,
    pub body: String,
}

pub fn accept_transition(
    user_id: u64,
    recruit_role_id: u64,
    next_rank_role_id: u64,
) -> RecruitRoleTransition {
    RecruitRoleTransition {
        user_id,
        remove_role_ids: vec![recruit_role_id],
        add_role_ids: vec![next_rank_role_id],
    }
}

pub fn reject_transition(
    user_id: u64,
    recruit_role_id: u64,
    clan_member_role_id: u64,
    guest_role_id: u64,
) -> RecruitRoleTransition {
    RecruitRoleTransition {
        user_id,
        remove_role_ids: vec![recruit_role_id, clan_member_role_id],
        add_role_ids: vec![guest_role_id],
    }
}

pub fn decision_dm(user_id: u64, body: &str) -> RecruitDmDraft {
    RecruitDmDraft {
        user_id,
        body: body.to_owned(),
    }
}

#[derive(Clone)]
pub struct RecruitDiscordHttp {
    client: Arc<DiscordHttpClient>,
}

impl RecruitDiscordHttp {
    pub fn new(client: Arc<DiscordHttpClient>) -> Self {
        Self { client }
    }

    pub async fn apply_role_transition(
        &self,
        guild_id: u64,
        transition: &RecruitRoleTransition,
    ) -> Result<(), String> {
        for role_id in &transition.remove_role_ids {
            self.client
                .remove_guild_member_role(
                    Id::<GuildMarker>::new(guild_id),
                    Id::<UserMarker>::new(transition.user_id),
                    Id::<RoleMarker>::new(*role_id),
                )
                .reason(RECRUIT_ROLE_AUDIT_REASON)
                .await
                .map(|_| ())
                .map_err(|err| {
                    format!(
                        "failed to remove recruit role {role_id} from {}: {err}",
                        transition.user_id
                    )
                })?;
        }
        for role_id in &transition.add_role_ids {
            self.client
                .add_guild_member_role(
                    Id::<GuildMarker>::new(guild_id),
                    Id::<UserMarker>::new(transition.user_id),
                    Id::<RoleMarker>::new(*role_id),
                )
                .reason(RECRUIT_ROLE_AUDIT_REASON)
                .await
                .map(|_| ())
                .map_err(|err| {
                    format!(
                        "failed to add recruit role {role_id} to {}: {err}",
                        transition.user_id
                    )
                })?;
        }
        Ok(())
    }

    pub async fn send_decision_panel(
        &self,
        channel_id: u64,
        content: &str,
        allowed_ping_role_ids: &[u64],
        embed: Option<Embed>,
        components: &[Component],
    ) -> Result<DiscordMessage, String> {
        let allowed_mentions = AllowedMentions {
            roles: allowed_ping_role_ids
                .iter()
                .copied()
                .map(Id::<RoleMarker>::new)
                .collect(),
            ..AllowedMentions::default()
        };
        let mut request = self
            .client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&allowed_mentions))
            .content(content)
            .components(components);
        let embeds;
        if let Some(embed) = embed {
            embeds = vec![embed];
            request = request.embeds(&embeds);
        }
        request
            .await
            .map_err(|err| format!("failed to send recruit decision panel: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode recruit decision panel message: {err}"))
    }

    pub async fn dm_user(&self, draft: &RecruitDmDraft) -> Result<Option<u64>, String> {
        let channel = self
            .client
            .create_private_channel(Id::<UserMarker>::new(draft.user_id))
            .await
            .map_err(|err| format!("failed to open recruit DM for {}: {err}", draft.user_id))?
            .model()
            .await
            .map_err(|err| format!("failed to decode recruit DM channel: {err}"))?;
        let message = self
            .client
            .create_message(channel.id)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .content(&draft.body)
            .await
            .map_err(|err| format!("failed to send recruit DM to {}: {err}", draft.user_id))?
            .model()
            .await
            .map_err(|err| format!("failed to decode recruit DM message: {err}"))?;
        Ok(Some(message.id.get()))
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
            .map_err(|err| format!("failed to respond to recruit interaction: {err}"))
    }
}
