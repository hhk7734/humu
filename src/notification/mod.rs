pub mod crypto;
pub mod os;
pub mod telegram;

use crate::config::NotificationsConfig;
use os::OsNotifier;
use telegram::TelegramNotifier;

pub enum NotificationEvent {
    AgentNeedsInput { workspace: String, room: String },
    AgentFinished { workspace: String, room: String },
}

impl NotificationEvent {
    pub fn message(&self) -> (&str, String) {
        match self {
            Self::AgentNeedsInput { workspace, room } => {
                ("Humu", format!("[{workspace}/{room}] Agent needs input"))
            }
            Self::AgentFinished { workspace, room } => {
                ("Humu", format!("[{workspace}/{room}] Agent finished"))
            }
        }
    }
}

pub struct NotificationManager {
    os: Option<OsNotifier>,
    telegram: Option<TelegramNotifier>,
}

impl NotificationManager {
    pub fn from_config(config: &NotificationsConfig) -> Self {
        let os = if config.os.enabled {
            Some(OsNotifier { sound: config.os.sound })
        } else {
            None
        };

        let telegram = if config.telegram.enabled {
            match (
                crypto::decrypt(&config.telegram.bot_token_encrypted),
                crypto::decrypt(&config.telegram.chat_id_encrypted),
            ) {
                (Ok(token), Ok(chat_id)) if !token.is_empty() && !chat_id.is_empty() => {
                    Some(TelegramNotifier::new(token, chat_id))
                }
                _ => {
                    crate::humu_log!("telegram notification enabled but credentials missing or invalid");
                    None
                }
            }
        } else {
            None
        };

        Self { os, telegram }
    }

    pub fn notify(&self, event: NotificationEvent) {
        let (title, body) = event.message();
        if let Some(os) = &self.os {
            os.send(title, &body);
        }
        if let Some(tg) = &self.telegram {
            tg.send(title, &body);
        }
    }
}
