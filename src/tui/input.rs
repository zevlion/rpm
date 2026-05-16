use crate::ipc::messages::DaemonCommand;
use crossterm::event::{KeyCode, KeyEvent};

pub enum Action {
    Quit,
    Command(DaemonCommand),
    SelectUp,
    SelectDown,
    None,
}

pub fn handle_key(key: KeyEvent, selected_id: Option<u32>) -> Action {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,

        KeyCode::Up | KeyCode::Char('k') => Action::SelectUp,
        KeyCode::Down | KeyCode::Char('j') => Action::SelectDown,

        KeyCode::Char('s') => {
            if let Some(id) = selected_id {
                Action::Command(DaemonCommand::Stop {
                    target: id.to_string(),
                })
            } else {
                Action::None
            }
        }

        KeyCode::Char('r') => {
            if let Some(id) = selected_id {
                Action::Command(DaemonCommand::Restart {
                    target: id.to_string(),
                })
            } else {
                Action::None
            }
        }

        KeyCode::Char('d') => {
            if let Some(id) = selected_id {
                Action::Command(DaemonCommand::Delete {
                    target: id.to_string(),
                })
            } else {
                Action::None
            }
        }

        KeyCode::Char('w') => {
            if let Some(id) = selected_id {
                Action::Command(DaemonCommand::Watch {
                    target: id.to_string(),
                    enable: true,
                })
            } else {
                Action::None
            }
        }

        _ => Action::None,
    }
}
