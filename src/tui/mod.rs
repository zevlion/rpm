pub mod input;
pub mod print;

use crate::ipc::{IpcClient, messages::DaemonCommand};
use crate::process::Process;
use anyhow::Result;
use crossterm::event::{self, Event};
use input::{Action, handle_key};
use print::Term;
use ratatui::widgets::TableState;
use std::time::Duration;

pub async fn run(terminal: &mut Term) -> Result<()> {
    let mut client = IpcClient::connect().await?;
    let mut processes: Vec<Process> = vec![];
    let mut table_state = TableState::default();
    table_state.select(Some(0));

    loop {
        // fetch latest process list
        if let Ok(res) = client.send(DaemonCommand::List).await {
            if let crate::ipc::messages::DaemonResponse::ProcessList(list) = res {
                processes = list;
            }
        }

        print::draw(terminal, &processes, &mut table_state)?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                let selected_id = table_state
                    .selected()
                    .and_then(|i| processes.get(i))
                    .map(|p| p.id);

                match handle_key(key, selected_id) {
                    Action::Quit => break,

                    Action::SelectUp => {
                        let i = table_state.selected().unwrap_or(0);
                        if i > 0 {
                            table_state.select(Some(i - 1));
                        }
                    }

                    Action::SelectDown => {
                        let i = table_state.selected().unwrap_or(0);
                        if i + 1 < processes.len() {
                            table_state.select(Some(i + 1));
                        }
                    }

                    Action::Command(cmd) => {
                        let _ = client.send(cmd).await;
                    }

                    Action::None => {}
                }
            }
        }
    }

    Ok(())
}
