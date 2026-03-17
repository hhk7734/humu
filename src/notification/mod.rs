pub mod crypto;
pub mod os;
pub mod telegram;

use crate::config::NotificationsConfig;
use os::{OsNotifier, SoundNotifier};
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

struct Channel<T> {
    notifier: T,
    only_unfocused: bool,
}

pub struct NotificationManager {
    os: Option<Channel<OsNotifier>>,
    sound: Option<Channel<SoundNotifier>>,
    telegram: Option<Channel<TelegramNotifier>>,
}

impl NotificationManager {
    pub fn from_config(config: &NotificationsConfig) -> Self {
        let os = if config.os.enabled {
            Some(Channel {
                notifier: OsNotifier,
                only_unfocused: config.os.only_unfocused,
            })
        } else {
            None
        };

        let sound = if config.sound.enabled {
            Some(Channel {
                notifier: SoundNotifier,
                only_unfocused: config.sound.only_unfocused,
            })
        } else {
            None
        };

        let telegram = if config.telegram.enabled {
            match (
                crypto::decrypt(&config.telegram.bot_token_encrypted),
                crypto::decrypt(&config.telegram.chat_id_encrypted),
            ) {
                (Ok(token), Ok(chat_id)) if !token.is_empty() && !chat_id.is_empty() => {
                    Some(Channel {
                        notifier: TelegramNotifier::new(token, chat_id),
                        only_unfocused: config.telegram.only_unfocused,
                    })
                }
                _ => {
                    crate::humu_log!("telegram notification enabled but credentials missing or invalid");
                    None
                }
            }
        } else {
            None
        };

        Self { os, sound, telegram }
    }

    pub fn notify(&self, event: NotificationEvent, focused: bool) {
        let (title, body) = event.message();

        if let Some(ch) = &self.os {
            if !focused || !ch.only_unfocused {
                ch.notifier.send(title, &body);
            }
        }
        if let Some(ch) = &self.sound {
            if !focused || !ch.only_unfocused {
                ch.notifier.send();
            }
        }
        if let Some(ch) = &self.telegram {
            if !focused || !ch.only_unfocused {
                ch.notifier.send(title, &body);
            }
        }
    }
}
