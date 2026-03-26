pub mod crypto;
pub mod os;
pub mod telegram;

use crate::config::NotificationsConfig;
use os::{OsNotifier, SoundNotifier};
use telegram::TelegramNotifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionFocusState {
    attached: bool,
    client_focused: bool,
}

impl Default for SessionFocusState {
    fn default() -> Self {
        Self::detached()
    }
}

impl SessionFocusState {
    pub fn attached() -> Self {
        Self {
            attached: true,
            client_focused: true,
        }
    }

    pub fn detached() -> Self {
        Self {
            attached: false,
            client_focused: false,
        }
    }

    pub fn update_client_focus(&mut self, focused: bool) {
        self.attached = true;
        self.client_focused = focused;
    }

    pub fn is_effectively_focused(self) -> bool {
        self.attached && self.client_focused
    }

    pub fn delivers_only_unfocused(self) -> bool {
        !self.is_effectively_focused()
    }
}

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
                    crate::humu_log!(
                        "telegram notification enabled but credentials missing or invalid"
                    );
                    None
                }
            }
        } else {
            None
        };

        Self {
            os,
            sound,
            telegram,
        }
    }

    pub fn notify(&self, event: NotificationEvent, focused: bool) {
        let focus_state = if focused {
            SessionFocusState::attached()
        } else {
            let mut focus_state = SessionFocusState::attached();
            focus_state.update_client_focus(false);
            focus_state
        };
        self.notify_with_session_focus(event, focus_state);
    }

    pub fn notify_with_session_focus(
        &self,
        event: NotificationEvent,
        focus_state: SessionFocusState,
    ) {
        let (title, body) = event.message();

        if let Some(ch) = &self.os {
            if focus_state.delivers_only_unfocused() || !ch.only_unfocused {
                ch.notifier.send(title, &body);
            }
        }
        if let Some(ch) = &self.sound {
            if focus_state.delivers_only_unfocused() || !ch.only_unfocused {
                ch.notifier.send();
            }
        }
        if let Some(ch) = &self.telegram {
            if focus_state.delivers_only_unfocused() || !ch.only_unfocused {
                ch.notifier.send(title, &body);
            }
        }
    }
}
