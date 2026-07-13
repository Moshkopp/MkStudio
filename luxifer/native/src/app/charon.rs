//! Hintergrundverbindung zum optionalen Charon-Dienst (ADR 0012).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use luxifer_application::CharonConnection;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct CharonConfig {
    url: String,
    workplace_id: String,
    workplace_name: String,
    settings: luxifer_core::UiSettings,
    lasers: luxifer_core::LaserRegistry,
}

enum WorkerCommand {
    Configure(Option<Box<CharonConfig>>),
    FetchBackups,
}

pub(super) enum CharonWorkerResult {
    Connected(
        CharonConnection,
        Result<luxifer_application::CharonSyncReport, String>,
    ),
    Failed(String),
    Disabled,
    Backups(Vec<luxifer_application::CharonWorkplaceBackup>),
}

pub(super) struct CharonRuntime {
    command_tx: Sender<WorkerCommand>,
    result_rx: Receiver<CharonWorkerResult>,
}

impl CharonRuntime {
    pub fn new(settings: &luxifer_core::UiSettings, lasers: &luxifer_core::LaserRegistry) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("charon-heartbeat".into())
            .spawn(move || worker(command_rx, result_tx))
            .expect("Charon-Hintergrundthread konnte nicht gestartet werden");
        let runtime = Self {
            command_tx,
            result_rx,
        };
        runtime.configure(settings, lasers);
        runtime
    }

    pub fn configure(
        &self,
        settings: &luxifer_core::UiSettings,
        lasers: &luxifer_core::LaserRegistry,
    ) {
        let config = settings.charon_enabled.then(|| {
            Box::new(CharonConfig {
                url: settings.charon_url.clone(),
                workplace_id: settings.workplace_id.clone(),
                workplace_name: settings.workplace.clone(),
                settings: settings.clone(),
                lasers: lasers.clone(),
            })
        });
        let _ = self.command_tx.send(WorkerCommand::Configure(config));
    }

    pub fn try_result(&self) -> Option<CharonWorkerResult> {
        self.result_rx.try_iter().last()
    }

    pub fn fetch_backups(&self) {
        let _ = self.command_tx.send(WorkerCommand::FetchBackups);
    }
}

fn worker(command_rx: Receiver<WorkerCommand>, result_tx: Sender<CharonWorkerResult>) {
    let mut config: Option<Box<CharonConfig>> = None;
    let mut event_cursor = 0_u64;
    let mut server_instance: Option<String> = None;
    loop {
        match config.as_ref() {
            Some(current) => {
                let current = current.clone();
                let connection = luxifer_application::connect_charon(
                    &current.url,
                    &current.workplace_id,
                    &current.workplace_name,
                );
                let connected = connection.is_ok();
                let result = connection
                    .map(|connection| {
                        if server_instance.as_deref()
                            != Some(connection.handshake.instance_id.as_str())
                        {
                            event_cursor = 0;
                            server_instance = Some(connection.handshake.instance_id.clone());
                        }
                        let sync = luxifer_application::sync_assets(&current.url)
                            .and_then(|mut report| {
                                report.backups_uploaded =
                                    luxifer_application::upload_workplace_backups(
                                        &current.url,
                                        &current.settings,
                                        &current.lasers,
                                    )?;
                                let projects = luxifer_application::sync_project_revisions(
                                    &current.url,
                                    &current.workplace_id,
                                )?;
                                report.uploaded += projects.uploaded;
                                report.pending += projects.pending;
                                report.received += projects.received;
                                Ok(report)
                            })
                            .map_err(|error| error.message().to_owned());
                        CharonWorkerResult::Connected(connection, sync)
                    })
                    .unwrap_or_else(|error| CharonWorkerResult::Failed(error.message().to_owned()));
                if result_tx.send(result).is_err() {
                    return;
                }
                if connected {
                    match luxifer_application::wait_for_project_event(
                        &current.url,
                        &current.workplace_id,
                        event_cursor,
                    ) {
                        Ok(event) => event_cursor = event.cursor,
                        Err(_) => match command_rx.recv_timeout(HEARTBEAT_INTERVAL) {
                            Ok(WorkerCommand::Configure(next)) => {
                                config = next;
                                event_cursor = 0;
                                server_instance = None;
                            }
                            Ok(WorkerCommand::FetchBackups) => {
                                send_backups(&current, &result_tx);
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => return,
                        },
                    }
                } else {
                    match command_rx.recv_timeout(HEARTBEAT_INTERVAL) {
                        Ok(WorkerCommand::Configure(next)) => {
                            config = next;
                            event_cursor = 0;
                            server_instance = None;
                        }
                        Ok(WorkerCommand::FetchBackups) => {
                            send_backups(&current, &result_tx);
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                match command_rx.try_recv() {
                    Ok(WorkerCommand::Configure(next)) => {
                        config = next;
                        event_cursor = 0;
                        server_instance = None;
                    }
                    Ok(WorkerCommand::FetchBackups) => {
                        send_backups(&current, &result_tx);
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            None => match command_rx.recv() {
                Ok(WorkerCommand::Configure(next)) => {
                    config = next;
                    event_cursor = 0;
                    server_instance = None;
                    if config.is_none() && result_tx.send(CharonWorkerResult::Disabled).is_err() {
                        return;
                    }
                }
                Ok(WorkerCommand::FetchBackups) => {}
                Err(_) => return,
            },
        }
    }
}

fn send_backups(config: &CharonConfig, result_tx: &Sender<CharonWorkerResult>) {
    let result = match luxifer_application::list_workplace_backups(&config.url) {
        Ok(backups) => CharonWorkerResult::Backups(backups),
        Err(error) => CharonWorkerResult::Failed(error.message().to_owned()),
    };
    let _ = result_tx.send(result);
}
