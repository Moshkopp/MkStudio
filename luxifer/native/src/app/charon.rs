//! Hintergrundverbindung zum optionalen Charon-Dienst (ADR 0012).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use luxifer_application::CharonConnection;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct CharonConfig {
    url: String,
    workplace_id: String,
    workplace_name: String,
}

enum WorkerCommand {
    Configure(Option<CharonConfig>),
}

pub(super) enum CharonWorkerResult {
    Connected(CharonConnection),
    Failed(String),
    Disabled,
}

pub(super) struct CharonRuntime {
    command_tx: Sender<WorkerCommand>,
    result_rx: Receiver<CharonWorkerResult>,
}

impl CharonRuntime {
    pub fn new(settings: &luxifer_core::UiSettings) -> Self {
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
        runtime.configure(settings);
        runtime
    }

    pub fn configure(&self, settings: &luxifer_core::UiSettings) {
        let config = settings.charon_enabled.then(|| CharonConfig {
            url: settings.charon_url.clone(),
            workplace_id: settings.workplace_id.clone(),
            workplace_name: settings.workplace.clone(),
        });
        let _ = self.command_tx.send(WorkerCommand::Configure(config));
    }

    pub fn try_result(&self) -> Option<CharonWorkerResult> {
        self.result_rx.try_iter().last()
    }
}

fn worker(command_rx: Receiver<WorkerCommand>, result_tx: Sender<CharonWorkerResult>) {
    let mut config: Option<CharonConfig> = None;
    loop {
        match config.as_ref() {
            Some(current) => {
                let result = luxifer_application::connect_charon(
                    &current.url,
                    &current.workplace_id,
                    &current.workplace_name,
                )
                .map(CharonWorkerResult::Connected)
                .unwrap_or_else(|error| CharonWorkerResult::Failed(error.message().to_owned()));
                if result_tx.send(result).is_err() {
                    return;
                }
                match command_rx.recv_timeout(HEARTBEAT_INTERVAL) {
                    Ok(WorkerCommand::Configure(next)) => config = next,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            None => match command_rx.recv() {
                Ok(WorkerCommand::Configure(next)) => {
                    config = next;
                    if config.is_none() && result_tx.send(CharonWorkerResult::Disabled).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            },
        }
    }
}
