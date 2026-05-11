use crate::commands::{validate_setup_voice_hub, SetupVoiceHubValidation};
use crate::repository::LegacySqliteTempVoiceRepository;
use crate::runtime::{
    hub_setup_transaction, plan_voice_state_update, reconcile_startup, room_create_transaction,
    room_delete_transaction, TempVoiceDbMutation,
};
use crate::state::{empty_owned_channel_action, HubSettings, TempVoiceAction, TempVoiceChannel};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn setup_requires_owner_or_admin_and_voice_channel() {
    assert_eq!(
        validate_setup_voice_hub(false, false, true),
        SetupVoiceHubValidation::Denied("only server owner or administrators can set the hub")
    );
    assert_eq!(
        validate_setup_voice_hub(false, true, false),
        SetupVoiceHubValidation::Denied("target channel must be a voice channel")
    );
    assert_eq!(
        validate_setup_voice_hub(false, true, true),
        SetupVoiceHubValidation::Allowed
    );
}

#[test]
fn delete_guard_only_deletes_tracked_empty_channels() {
    let tracked = vec![TempVoiceChannel {
        channel_id: 10,
        guild_id: 1,
        owner_user_id: 42,
        member_count: 0,
    }];

    assert_eq!(
        empty_owned_channel_action(&tracked, 10, 0),
        TempVoiceAction::DeleteOwnedChannel { channel_id: 10 }
    );
    assert_eq!(
        empty_owned_channel_action(&tracked, 99, 0),
        TempVoiceAction::Ignore
    );
}

#[test]
fn joining_hub_creates_room_and_leaving_empty_owned_channel_deletes() {
    let hub = HubSettings {
        guild_id: 1,
        hub_channel_id: 5,
    };
    let tracked = vec![TempVoiceChannel {
        channel_id: 10,
        guild_id: 1,
        owner_user_id: 42,
        member_count: 0,
    }];

    let actions = plan_voice_state_update(&hub, &tracked, 42, Some(5), Some(10), 0);

    assert_eq!(actions.len(), 2);
    assert!(matches!(actions[0], TempVoiceAction::CreateRoom { .. }));
    assert_eq!(
        actions[1],
        TempVoiceAction::DeleteOwnedChannel { channel_id: 10 }
    );
}

#[test]
fn startup_reconciliation_reports_missing_and_empty_owned_channels() {
    let tracked = vec![
        TempVoiceChannel {
            channel_id: 10,
            guild_id: 1,
            owner_user_id: 42,
            member_count: 0,
        },
        TempVoiceChannel {
            channel_id: 11,
            guild_id: 1,
            owner_user_id: 43,
            member_count: 1,
        },
    ];

    let plan = reconcile_startup(&tracked, &[10]);

    assert_eq!(plan.empty_owned_channels, vec![10]);
    assert_eq!(plan.missing_in_discord, vec![11]);
}

#[test]
fn delayed_delete_is_scheduled_for_tracked_empty_channel() {
    let tracked = vec![TempVoiceChannel {
        channel_id: 10,
        guild_id: 1,
        owner_user_id: 42,
        member_count: 0,
    }];

    assert_eq!(
        empty_owned_channel_action(&tracked, 10, 30),
        TempVoiceAction::ScheduleDeleteOwnedChannel {
            channel_id: 10,
            delay_seconds: 30
        }
    );
}

#[test]
fn db_mutations_are_explicit_atomic_transaction_plans() {
    let setup = hub_setup_transaction(1, 5);
    assert!(setup.must_be_atomic);
    assert_eq!(
        setup.mutations,
        vec![TempVoiceDbMutation::UpsertHub {
            guild_id: 1,
            hub_channel_id: 5
        }]
    );

    let create = room_create_transaction(10, 1, 42);
    assert!(create.must_be_atomic);
    assert!(matches!(
        create.mutations.as_slice(),
        [TempVoiceDbMutation::InsertTempChannel {
            channel_id: 10,
            guild_id: 1,
            owner_user_id: 42
        }]
    ));

    let delete = room_delete_transaction(10);
    assert!(delete.must_be_atomic);
    assert_eq!(
        delete.mutations,
        vec![TempVoiceDbMutation::DeleteTrackedChannel { channel_id: 10 }]
    );
}

#[tokio::test]
async fn sqlite_repository_writes_hub_and_temp_channel_transactions() {
    let path = temp_db_path("temp_voice_repo_writes");
    let repository = LegacySqliteTempVoiceRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repository.create_schema_for_tests().await.unwrap();

    repository.set_guild_hub_channel(1, 5).await.unwrap();
    assert_eq!(repository.get_guild_hub_channel(1).await.unwrap(), Some(5));

    repository.insert_temp_channel(10, 1, 42).await.unwrap();
    let records = repository.list_temp_channels_by_guild(1).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].channel_id, 10);
    assert_eq!(records[0].owner_user_id, 42);

    repository.remove_temp_channel(10).await.unwrap();
    assert!(repository.get_temp_channel(10).await.unwrap().is_none());
    cleanup_db(path);
}

#[tokio::test]
async fn sqlite_repository_read_only_rejects_writes() {
    let path = temp_db_path("temp_voice_repo_read_only");
    let repository = LegacySqliteTempVoiceRepository::open_writable_for_tests(&path)
        .await
        .unwrap();
    repository.create_schema_for_tests().await.unwrap();
    drop(repository);

    let repository = LegacySqliteTempVoiceRepository::open_existing_read_only(&path)
        .await
        .unwrap();
    let err = repository.set_guild_hub_channel(1, 5).await.unwrap_err();
    assert!(err.contains("read-only"));
    cleanup_db(path);
}

fn temp_db_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "xiii_superbot_{name}_{}.sqlite3",
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst) ^ nanos as u64
    ));
    let _ = fs::remove_file(&path);
    path
}

fn cleanup_db(path: PathBuf) {
    let _ = fs::remove_file(path);
}
