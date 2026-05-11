use crate::discord_io::{is_regular_voice_channel, room_name, TempVoiceDiscordHttp};
use crate::repository::LegacySqliteTempVoiceRepository;
use crate::state::{
    empty_owned_channel_action, user_joined_hub, HubSettings, TempVoiceAction, TempVoiceChannel,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupReconciliation {
    pub missing_in_discord: Vec<u64>,
    pub empty_owned_channels: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempVoiceDbMutation {
    UpsertHub {
        guild_id: u64,
        hub_channel_id: u64,
    },
    InsertTempChannel {
        channel_id: u64,
        guild_id: u64,
        owner_user_id: u64,
    },
    DeleteTrackedChannel {
        channel_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempVoiceDbTransactionPlan {
    pub mutations: Vec<TempVoiceDbMutation>,
    pub must_be_atomic: bool,
}

impl TempVoiceDbTransactionPlan {
    pub fn new(mutations: Vec<TempVoiceDbMutation>) -> Self {
        Self {
            mutations,
            must_be_atomic: true,
        }
    }
}

pub fn plan_voice_state_update(
    hub: &HubSettings,
    tracked: &[TempVoiceChannel],
    user_id: u64,
    new_channel_id: Option<u64>,
    old_channel_id: Option<u64>,
    delete_after_seconds: u64,
) -> Vec<TempVoiceAction> {
    let mut actions = vec![user_joined_hub(hub, user_id, new_channel_id)];
    if let Some(old_channel_id) = old_channel_id {
        actions.push(empty_owned_channel_action(
            tracked,
            old_channel_id,
            delete_after_seconds,
        ));
    }
    actions
        .into_iter()
        .filter(|action| !matches!(action, TempVoiceAction::Ignore))
        .collect()
}

pub fn reconcile_startup(
    tracked: &[TempVoiceChannel],
    live_channel_ids: &[u64],
) -> StartupReconciliation {
    StartupReconciliation {
        missing_in_discord: tracked
            .iter()
            .filter(|channel| !live_channel_ids.contains(&channel.channel_id))
            .map(|channel| channel.channel_id)
            .collect(),
        empty_owned_channels: tracked
            .iter()
            .filter(|channel| {
                channel.member_count == 0 && live_channel_ids.contains(&channel.channel_id)
            })
            .map(|channel| channel.channel_id)
            .collect(),
    }
}

pub fn hub_setup_transaction(guild_id: u64, hub_channel_id: u64) -> TempVoiceDbTransactionPlan {
    TempVoiceDbTransactionPlan::new(vec![TempVoiceDbMutation::UpsertHub {
        guild_id,
        hub_channel_id,
    }])
}

pub fn room_create_transaction(
    channel_id: u64,
    guild_id: u64,
    owner_user_id: u64,
) -> TempVoiceDbTransactionPlan {
    TempVoiceDbTransactionPlan::new(vec![TempVoiceDbMutation::InsertTempChannel {
        channel_id,
        guild_id,
        owner_user_id,
    }])
}

pub fn room_delete_transaction(channel_id: u64) -> TempVoiceDbTransactionPlan {
    TempVoiceDbTransactionPlan::new(vec![TempVoiceDbMutation::DeleteTrackedChannel {
        channel_id,
    }])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempVoiceSetupOutcome {
    pub guild_id: u64,
    pub hub_channel_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempVoiceCreateOutcome {
    pub guild_id: u64,
    pub owner_user_id: u64,
    pub created_channel_id: u64,
    pub moved_user: bool,
    pub inserted_db_record: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempVoiceDeleteOutcome {
    Deleted {
        channel_id: u64,
    },
    KeptOccupied {
        channel_id: u64,
        member_count: usize,
    },
    IgnoredUntracked {
        channel_id: u64,
    },
    CleanedMissing {
        channel_id: u64,
    },
}

#[derive(Clone)]
pub struct TempVoiceRuntime {
    repository: LegacySqliteTempVoiceRepository,
    discord: TempVoiceDiscordHttp,
}

impl TempVoiceRuntime {
    pub fn new(repository: LegacySqliteTempVoiceRepository, discord: TempVoiceDiscordHttp) -> Self {
        Self {
            repository,
            discord,
        }
    }

    pub async fn setup_voice_hub(
        &self,
        guild_id: u64,
        hub_channel_id: u64,
    ) -> Result<TempVoiceSetupOutcome, String> {
        let channel = self.discord.get_channel(hub_channel_id).await?;
        if channel.guild_id.map(|id| id.get()) != Some(guild_id) {
            return Err("This channel does not belong to this server.".to_owned());
        }
        if !is_regular_voice_channel(&channel) {
            return Err("This channel is not a regular voice channel.".to_owned());
        }
        self.repository
            .set_guild_hub_channel(guild_id, hub_channel_id)
            .await?;
        Ok(TempVoiceSetupOutcome {
            guild_id,
            hub_channel_id,
        })
    }

    pub async fn repository_hub_for_guild(&self, guild_id: u64) -> Result<Option<u64>, String> {
        self.repository.get_guild_hub_channel(guild_id).await
    }

    pub async fn respond_interaction_ephemeral(
        &self,
        application_id: u64,
        interaction_id: u64,
        token: &str,
        content: &str,
    ) -> Result<(), String> {
        self.discord
            .respond_interaction_ephemeral(application_id, interaction_id, token, content)
            .await
    }

    pub async fn create_room_for_hub_join(
        &self,
        guild_id: u64,
        hub_channel_id: u64,
        owner_user_id: u64,
        display_name: &str,
    ) -> Result<TempVoiceCreateOutcome, String> {
        let hub = self.discord.get_channel(hub_channel_id).await?;
        if !is_regular_voice_channel(&hub) {
            return Err("configured hub is not a regular voice channel".to_owned());
        }
        let name = room_name(display_name);
        let created = self
            .discord
            .create_voice_channel_from_hub(guild_id, &hub, &name)
            .await?;
        let created_channel_id = created.id.get();

        if let Err(move_err) = self
            .discord
            .move_member_to_channel(guild_id, owner_user_id, created_channel_id)
            .await
        {
            let _ = self.discord.delete_channel(created_channel_id).await;
            return Err(move_err);
        }

        if let Err(insert_err) = self
            .repository
            .insert_temp_channel(created_channel_id, guild_id, owner_user_id)
            .await
        {
            let _ = self.discord.delete_channel(created_channel_id).await;
            return Err(insert_err);
        }

        Ok(TempVoiceCreateOutcome {
            guild_id,
            owner_user_id,
            created_channel_id,
            moved_user: true,
            inserted_db_record: true,
        })
    }

    pub async fn delete_tracked_channel_if_empty(
        &self,
        channel_id: u64,
        member_count: usize,
    ) -> Result<TempVoiceDeleteOutcome, String> {
        let Some(record) = self.repository.get_temp_channel(channel_id).await? else {
            return Ok(TempVoiceDeleteOutcome::IgnoredUntracked { channel_id });
        };

        if member_count > 0 {
            self.repository
                .update_last_empty_at(channel_id, None)
                .await?;
            return Ok(TempVoiceDeleteOutcome::KeptOccupied {
                channel_id,
                member_count,
            });
        }

        match self.discord.get_channel_optional(channel_id).await? {
            Some(channel) => {
                if channel.id.get() == record.channel_id {
                    self.discord.delete_channel(channel_id).await?;
                }
                self.repository.remove_temp_channel(channel_id).await?;
                Ok(TempVoiceDeleteOutcome::Deleted { channel_id })
            }
            None => {
                self.repository.remove_temp_channel(channel_id).await?;
                Ok(TempVoiceDeleteOutcome::CleanedMissing { channel_id })
            }
        }
    }

    pub async fn mark_last_empty_at(
        &self,
        channel_id: u64,
        last_empty_at: Option<String>,
    ) -> Result<(), String> {
        self.repository
            .update_last_empty_at(channel_id, last_empty_at)
            .await
    }

    pub async fn startup_reconcile(
        &self,
        live_member_count: impl Fn(u64) -> usize,
    ) -> Result<Vec<TempVoiceDeleteOutcome>, String> {
        let tracked = self.repository.list_temp_channels().await?;
        let mut outcomes = Vec::with_capacity(tracked.len());

        for channel in tracked {
            match self
                .discord
                .get_channel_optional(channel.channel_id)
                .await?
            {
                None => {
                    self.repository
                        .remove_temp_channel(channel.channel_id)
                        .await?;
                    outcomes.push(TempVoiceDeleteOutcome::CleanedMissing {
                        channel_id: channel.channel_id,
                    });
                }
                Some(_) => {
                    let member_count = live_member_count(channel.channel_id);
                    if member_count == 0 {
                        outcomes.push(
                            self.delete_tracked_channel_if_empty(channel.channel_id, 0)
                                .await?,
                        );
                    } else {
                        outcomes.push(TempVoiceDeleteOutcome::KeptOccupied {
                            channel_id: channel.channel_id,
                            member_count,
                        });
                    }
                }
            }
        }

        Ok(outcomes)
    }
}
