pub const MAX_IPC_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserIpcMessage {
    UpdateTitle { tab_id: u32, title: String },
    UpdateUrl { tab_id: u32, url: String },
    FocusOmnibox { tab_id: u32 },
    SaveConfig { json: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserIpcEnvelope {
    pub source_context: IpcSourceContext,
    pub payload: String,
}

impl BrowserIpcEnvelope {
    pub fn trusted_tab_event(tab_id: u32, payload: String) -> Self {
        Self {
            source_context: IpcSourceContext::TrustedTabEvent { tab_id },
            payload,
        }
    }

    pub fn content_webview(tab_id: u32, payload: String) -> Self {
        Self {
            source_context: IpcSourceContext::ContentWebView { tab_id },
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcSourceContext {
    TrustedTabEvent { tab_id: u32 },
    ContentWebView { tab_id: u32 },
    SettingsWebView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcParseError {
    Empty,
    TooLarge { actual: usize, max: usize },
    Malformed,
    InvalidTabId,
    UnknownCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcAuthorizationError {
    TabIdMismatch {
        source_tab_id: u32,
        message_tab_id: u32,
    },
    InactiveTab {
        source_tab_id: u32,
        active_tab_id: Option<u32>,
    },
    CommandNotAllowedFromContent,
    CommandNotAllowedFromSettings,
    CommandNotAllowedFromTrustedTabEvent,
}

pub fn parse_ipc_message(input: &str) -> Result<BrowserIpcMessage, IpcParseError> {
    if input.is_empty() {
        return Err(IpcParseError::Empty);
    }

    let len = input.len();
    if len > MAX_IPC_MESSAGE_BYTES {
        return Err(IpcParseError::TooLarge {
            actual: len,
            max: MAX_IPC_MESSAGE_BYTES,
        });
    }

    if let Some(json) = input.strip_prefix("save_config:") {
        return Ok(BrowserIpcMessage::SaveConfig {
            json: json.to_string(),
        });
    }

    let mut parts = input.splitn(3, '|');
    let tab_id = parts.next().ok_or(IpcParseError::Malformed)?;
    let command = parts.next().ok_or(IpcParseError::Malformed)?;
    let payload = parts.next().ok_or(IpcParseError::Malformed)?;

    if tab_id.is_empty() || command.is_empty() {
        return Err(IpcParseError::Malformed);
    }

    let tab_id = tab_id
        .parse::<u32>()
        .map_err(|_| IpcParseError::InvalidTabId)?;

    match command {
        "title" => Ok(BrowserIpcMessage::UpdateTitle {
            tab_id,
            title: payload.to_string(),
        }),
        "url" => Ok(BrowserIpcMessage::UpdateUrl {
            tab_id,
            url: payload.to_string(),
        }),
        "focus_omnibox" => {
            if payload.is_empty() {
                Ok(BrowserIpcMessage::FocusOmnibox { tab_id })
            } else {
                Err(IpcParseError::Malformed)
            }
        }
        _ => Err(IpcParseError::UnknownCommand),
    }
}

pub fn authorize_ipc_message(
    message: &BrowserIpcMessage,
    active_tab_id: Option<u32>,
    source_context: IpcSourceContext,
) -> Result<(), IpcAuthorizationError> {
    match source_context {
        IpcSourceContext::TrustedTabEvent { tab_id } => match message {
            BrowserIpcMessage::UpdateUrl {
                tab_id: message_tab_id,
                ..
            } => require_matching_tab(tab_id, *message_tab_id),
            _ => Err(IpcAuthorizationError::CommandNotAllowedFromTrustedTabEvent),
        },
        IpcSourceContext::ContentWebView { tab_id } => match message {
            BrowserIpcMessage::UpdateTitle {
                tab_id: message_tab_id,
                ..
            } => require_matching_tab(tab_id, *message_tab_id),
            BrowserIpcMessage::UpdateUrl {
                tab_id: message_tab_id,
                ..
            } => {
                require_matching_tab(tab_id, *message_tab_id)?;
                Err(IpcAuthorizationError::CommandNotAllowedFromContent)
            }
            BrowserIpcMessage::FocusOmnibox {
                tab_id: message_tab_id,
            } => {
                require_matching_tab(tab_id, *message_tab_id)?;
                if active_tab_id != Some(tab_id) {
                    return Err(IpcAuthorizationError::InactiveTab {
                        source_tab_id: tab_id,
                        active_tab_id,
                    });
                }
                Err(IpcAuthorizationError::CommandNotAllowedFromContent)
            }
            BrowserIpcMessage::SaveConfig { .. } => {
                Err(IpcAuthorizationError::CommandNotAllowedFromContent)
            }
        },
        IpcSourceContext::SettingsWebView => match message {
            BrowserIpcMessage::SaveConfig { .. } => Ok(()),
            _ => Err(IpcAuthorizationError::CommandNotAllowedFromSettings),
        },
    }
}

fn require_matching_tab(
    source_tab_id: u32,
    message_tab_id: u32,
) -> Result<(), IpcAuthorizationError> {
    if source_tab_id == message_tab_id {
        Ok(())
    } else {
        Err(IpcAuthorizationError::TabIdMismatch {
            source_tab_id,
            message_tab_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_message() {
        assert_eq!(
            parse_ipc_message("42|title|Petal Browser"),
            Ok(BrowserIpcMessage::UpdateTitle {
                tab_id: 42,
                title: "Petal Browser".to_string(),
            })
        );
    }

    #[test]
    fn parses_url_message_with_separator_in_payload() {
        assert_eq!(
            parse_ipc_message("7|url|https://example.test/a|b"),
            Ok(BrowserIpcMessage::UpdateUrl {
                tab_id: 7,
                url: "https://example.test/a|b".to_string(),
            })
        );
    }

    #[test]
    fn parses_focus_omnibox_message() {
        assert_eq!(
            parse_ipc_message("3|focus_omnibox|"),
            Ok(BrowserIpcMessage::FocusOmnibox { tab_id: 3 })
        );
    }

    #[test]
    fn parses_save_config_message() {
        assert_eq!(
            parse_ipc_message("save_config:{\"hardware_acceleration\":true}"),
            Ok(BrowserIpcMessage::SaveConfig {
                json: "{\"hardware_acceleration\":true}".to_string(),
            })
        );
    }

    #[test]
    fn rejects_empty_message() {
        assert_eq!(parse_ipc_message(""), Err(IpcParseError::Empty));
    }

    #[test]
    fn rejects_unknown_command() {
        assert_eq!(
            parse_ipc_message("1|open_devtools|"),
            Err(IpcParseError::UnknownCommand)
        );
    }

    #[test]
    fn rejects_payload_larger_than_limit() {
        let input = "x".repeat(MAX_IPC_MESSAGE_BYTES + 1);
        assert_eq!(
            parse_ipc_message(&input),
            Err(IpcParseError::TooLarge {
                actual: MAX_IPC_MESSAGE_BYTES + 1,
                max: MAX_IPC_MESSAGE_BYTES,
            })
        );
    }

    #[test]
    fn rejects_missing_payload() {
        assert_eq!(parse_ipc_message("1|title"), Err(IpcParseError::Malformed));
    }

    #[test]
    fn rejects_missing_tab_id() {
        assert_eq!(parse_ipc_message("|title|x"), Err(IpcParseError::Malformed));
    }

    #[test]
    fn rejects_invalid_tab_id() {
        assert_eq!(
            parse_ipc_message("abc|title|x"),
            Err(IpcParseError::InvalidTabId)
        );
    }

    #[test]
    fn rejects_unexpected_focus_payload() {
        assert_eq!(
            parse_ipc_message("1|focus_omnibox|unexpected|tail"),
            Err(IpcParseError::Malformed)
        );
    }

    #[test]
    fn allows_title_from_matching_content_tab() {
        let message = BrowserIpcMessage::UpdateTitle {
            tab_id: 2,
            title: "Example".to_string(),
        };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(2),
                IpcSourceContext::ContentWebView { tab_id: 2 }
            ),
            Ok(())
        );
    }

    #[test]
    fn allows_url_from_matching_trusted_tab_event() {
        let message = BrowserIpcMessage::UpdateUrl {
            tab_id: 2,
            url: "https://example.test".to_string(),
        };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(2),
                IpcSourceContext::TrustedTabEvent { tab_id: 2 }
            ),
            Ok(())
        );
    }

    #[test]
    fn rejects_mismatched_tab_id() {
        let message = BrowserIpcMessage::UpdateTitle {
            tab_id: 9,
            title: "Spoof".to_string(),
        };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(2),
                IpcSourceContext::ContentWebView { tab_id: 2 }
            ),
            Err(IpcAuthorizationError::TabIdMismatch {
                source_tab_id: 2,
                message_tab_id: 9,
            })
        );
    }

    #[test]
    fn rejects_settings_command_from_content_tab() {
        let message = BrowserIpcMessage::SaveConfig {
            json: "{}".to_string(),
        };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(2),
                IpcSourceContext::ContentWebView { tab_id: 2 }
            ),
            Err(IpcAuthorizationError::CommandNotAllowedFromContent)
        );
    }

    #[test]
    fn rejects_url_spoofing_from_content_tab() {
        let message = BrowserIpcMessage::UpdateUrl {
            tab_id: 2,
            url: "https://spoofed.test".to_string(),
        };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(2),
                IpcSourceContext::ContentWebView { tab_id: 2 }
            ),
            Err(IpcAuthorizationError::CommandNotAllowedFromContent)
        );
    }

    #[test]
    fn rejects_focus_omnibox_from_content_tab() {
        let message = BrowserIpcMessage::FocusOmnibox { tab_id: 2 };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(2),
                IpcSourceContext::ContentWebView { tab_id: 2 }
            ),
            Err(IpcAuthorizationError::CommandNotAllowedFromContent)
        );
    }

    #[test]
    fn rejects_focus_omnibox_from_inactive_content_tab() {
        let message = BrowserIpcMessage::FocusOmnibox { tab_id: 2 };

        assert_eq!(
            authorize_ipc_message(
                &message,
                Some(3),
                IpcSourceContext::ContentWebView { tab_id: 2 }
            ),
            Err(IpcAuthorizationError::InactiveTab {
                source_tab_id: 2,
                active_tab_id: Some(3),
            })
        );
    }

    #[test]
    fn allows_save_config_from_settings() {
        let message = BrowserIpcMessage::SaveConfig {
            json: "{}".to_string(),
        };

        assert_eq!(
            authorize_ipc_message(&message, Some(2), IpcSourceContext::SettingsWebView),
            Ok(())
        );
    }
}
