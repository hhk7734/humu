use humu::notification::{NotificationEvent, NotificationManager};
use humu::config::{NotificationsConfig, OsNotificationConfig, SoundNotificationConfig, TelegramNotificationConfig};

#[test]
fn notification_event_message_needs_input() {
    let event = NotificationEvent::AgentNeedsInput {
        workspace: "myproject".to_string(),
        room: "main".to_string(),
    };
    let (title, body) = event.message();
    assert_eq!(title, "Humu");
    assert_eq!(body, "[myproject/main] Agent needs input");
}

#[test]
fn notification_event_message_finished() {
    let event = NotificationEvent::AgentFinished {
        workspace: "myproject".to_string(),
        room: "dev".to_string(),
    };
    let (title, body) = event.message();
    assert_eq!(title, "Humu");
    assert_eq!(body, "[myproject/dev] Agent finished");
}

#[test]
fn manager_with_all_disabled_does_not_panic() {
    let config = NotificationsConfig {
        os: OsNotificationConfig { enabled: false, only_unfocused: true },
        sound: SoundNotificationConfig { enabled: false, only_unfocused: false },
        telegram: TelegramNotificationConfig::default(),
    };
    let manager = NotificationManager::from_config(&config);
    manager.notify(NotificationEvent::AgentFinished {
        workspace: "ws".to_string(),
        room: "rm".to_string(),
    }, true);
}
