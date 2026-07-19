//! Integration tests for the settings commands and the scheduler interval
//! push, plus the second POP3 delete guard in `set_pop3_delete_policy`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use axomail_core::MailError;
use axomail_desktop_lib::commands::accounts::create_account_impl;
use axomail_desktop_lib::commands::settings::{
    get_settings_impl, set_pop3_delete_policy_impl, update_settings_impl,
};
use axomail_desktop_lib::composition::AppProviderFactory;
use axomail_desktop_lib::dto::{Pop3DeletePolicyDto, SettingsDto, ThemePreference};
use axomail_desktop_lib::scheduler::SchedulerControl;
use axomail_desktop_lib::settings::pop3_delete_enabled;
use axomail_desktop_lib::state::{AppSyncEngine, SharedSecretStore};
use axomail_storage::Store;
use axomail_sync::{SyncConfig, SyncEngine};

use common::{pop3_account_input, secret_stores, valid_input};

/// Builds a real engine and scheduler control over `store`.
fn scheduler_for(store: &Store, secrets: &SharedSecretStore) -> SchedulerControl {
    let factory = AppProviderFactory::new(secrets.clone(), store.clone());
    let engine: Arc<AppSyncEngine> = Arc::new(SyncEngine::start(
        store.clone(),
        factory,
        SyncConfig::default(),
    ));
    SchedulerControl::new(engine, store.clone())
}

/// A settings DTO with no POP3 policies and the default theme.
fn settings_dto(notifications: bool, interval: u32) -> SettingsDto {
    SettingsDto {
        notifications_enabled: notifications,
        sync_interval_minutes: interval,
        theme: ThemePreference::System,
        pop3_delete_policies: Vec::new(),
    }
}

#[tokio::test]
async fn defaults_are_returned_when_nothing_is_stored() {
    let store = Store::open_in_memory().await.unwrap();

    let got = get_settings_impl(&store).await.unwrap();

    assert!(got.notifications_enabled, "notifications default on");
    assert_eq!(got.sync_interval_minutes, 10);
    assert_eq!(
        got.theme,
        ThemePreference::System,
        "theme defaults to system"
    );
    assert!(got.pop3_delete_policies.is_empty());
}

#[tokio::test]
async fn update_round_trips_the_theme_preference() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let scheduler = scheduler_for(&store, &secrets);

    let mut dto = settings_dto(true, 10);
    dto.theme = ThemePreference::Dark;
    update_settings_impl(&store, &scheduler, dto).await.unwrap();
    assert_eq!(
        get_settings_impl(&store).await.unwrap().theme,
        ThemePreference::Dark,
    );

    // Switching back to light persists just as well.
    let mut light = settings_dto(true, 10);
    light.theme = ThemePreference::Light;
    update_settings_impl(&store, &scheduler, light)
        .await
        .unwrap();
    assert_eq!(
        get_settings_impl(&store).await.unwrap().theme,
        ThemePreference::Light,
    );
}

#[tokio::test]
async fn update_round_trips_clamps_and_pushes_interval_to_scheduler() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let scheduler = scheduler_for(&store, &secrets);

    // A zero interval is clamped to the one-minute floor.
    update_settings_impl(&store, &scheduler, settings_dto(false, 0))
        .await
        .unwrap();
    let status = scheduler.status().expect("scheduler launched by update");
    assert_eq!(status.interval_secs, 60, "0 minutes clamps to 1");

    let got = get_settings_impl(&store).await.unwrap();
    assert!(!got.notifications_enabled);
    assert_eq!(got.sync_interval_minutes, 1);

    // A larger interval propagates to the scheduler status.
    update_settings_impl(&store, &scheduler, settings_dto(true, 5))
        .await
        .unwrap();
    assert_eq!(scheduler.status().unwrap().interval_secs, 300);
    assert!(
        get_settings_impl(&store)
            .await
            .unwrap()
            .notifications_enabled
    );
}

#[tokio::test]
async fn set_pop3_delete_policy_rejects_non_pop3_and_unknown_accounts() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();

    let imap = create_account_impl(&store, &secrets, valid_input())
        .await
        .unwrap();
    let imap_error = set_pop3_delete_policy_impl(&store, imap.id, true)
        .await
        .expect_err("an IMAP account has no POP3 delete policy");
    assert!(
        matches!(imap_error, MailError::UnsupportedCapability { .. }),
        "got {imap_error:?}"
    );

    let unknown = set_pop3_delete_policy_impl(&store, 9_999, true)
        .await
        .expect_err("an unknown account is rejected");
    assert!(matches!(unknown, MailError::Configuration { .. }));
}

#[tokio::test]
async fn delete_policy_is_set_only_by_the_dedicated_command() {
    let store = Store::open_in_memory().await.unwrap();
    let (_concrete, secrets) = secret_stores();
    let scheduler = scheduler_for(&store, &secrets);

    let pop = create_account_impl(&store, &secrets, pop3_account_input(995))
        .await
        .unwrap();

    // Enabling deletion is only possible through the explicit command.
    set_pop3_delete_policy_impl(&store, pop.id, true)
        .await
        .unwrap();
    assert!(pop3_delete_enabled(&store, pop.id).await.unwrap());

    // get_settings surfaces the policy for display.
    let got = get_settings_impl(&store).await.unwrap();
    let policy = got
        .pop3_delete_policies
        .iter()
        .find(|entry| entry.account_id == pop.id)
        .expect("policy is listed");
    assert!(policy.enabled);

    // update_settings must NOT be able to change server deletion, even when
    // its DTO claims to disable it.
    let dto = SettingsDto {
        notifications_enabled: true,
        sync_interval_minutes: 10,
        theme: ThemePreference::System,
        pop3_delete_policies: vec![Pop3DeletePolicyDto::new(pop.id, false).unwrap()],
    };
    update_settings_impl(&store, &scheduler, dto).await.unwrap();
    assert!(
        pop3_delete_enabled(&store, pop.id).await.unwrap(),
        "update_settings must not touch the delete policy"
    );
}
