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

enum LeaseCommand {
    Configure(Option<Box<CharonConfig>>),
    Acquire {
        controller_id: String,
        controller_name: String,
        force: bool,
    },
    Release,
    Usage(luxifer_application::LeaseUsage),
}

pub(super) enum LeaseWorkerResult {
    Acquired,
    Denied(luxifer_application::CharonLease),
    Released,
    ReleaseRequested,
    Lost(String),
}

struct ActiveLease {
    controller_id: String,
    token: String,
    usage: luxifer_application::LeaseUsage,
}

struct PendingLease {
    controller_id: String,
    controller_name: String,
}

pub(super) struct CharonRuntime {
    command_tx: Sender<WorkerCommand>,
    result_rx: Receiver<CharonWorkerResult>,
    lease_tx: Sender<LeaseCommand>,
    lease_rx: Receiver<LeaseWorkerResult>,
}

impl CharonRuntime {
    pub fn new(settings: &luxifer_core::UiSettings, lasers: &luxifer_core::LaserRegistry) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let (lease_tx, lease_command_rx) = mpsc::channel();
        let (lease_result_tx, lease_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("charon-heartbeat".into())
            .spawn(move || worker(command_rx, result_tx))
            .expect("Charon-Hintergrundthread konnte nicht gestartet werden");
        std::thread::Builder::new()
            .name("charon-lease".into())
            .spawn(move || lease_worker(lease_command_rx, lease_result_tx))
            .expect("Charon-Lease-Thread konnte nicht gestartet werden");
        let runtime = Self {
            command_tx,
            result_rx,
            lease_tx,
            lease_rx,
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
        let lease_config = settings.charon_enabled.then(|| {
            Box::new(CharonConfig {
                url: settings.charon_url.clone(),
                workplace_id: settings.workplace_id.clone(),
                workplace_name: settings.workplace.clone(),
                settings: settings.clone(),
                lasers: lasers.clone(),
            })
        });
        let _ = self.lease_tx.send(LeaseCommand::Configure(lease_config));
    }

    pub fn try_result(&self) -> Option<CharonWorkerResult> {
        self.result_rx.try_iter().last()
    }

    pub fn fetch_backups(&self) {
        let _ = self.command_tx.send(WorkerCommand::FetchBackups);
    }

    pub fn acquire_lease(&self, controller_id: String, controller_name: String, force: bool) {
        let _ = self.lease_tx.send(LeaseCommand::Acquire {
            controller_id,
            controller_name,
            force,
        });
    }

    pub fn release_lease(&self) {
        let _ = self.lease_tx.send(LeaseCommand::Release);
    }

    pub fn set_lease_usage(&self, usage: luxifer_application::LeaseUsage) {
        let _ = self.lease_tx.send(LeaseCommand::Usage(usage));
    }

    pub fn try_lease_result(&self) -> Option<LeaseWorkerResult> {
        self.lease_rx.try_iter().last()
    }
}

fn lease_worker(command_rx: Receiver<LeaseCommand>, result_tx: Sender<LeaseWorkerResult>) {
    let mut config: Option<Box<CharonConfig>> = None;
    let mut active: Option<ActiveLease> = None;
    let mut pending: Option<PendingLease> = None;
    loop {
        match command_rx.recv_timeout(HEARTBEAT_INTERVAL) {
            Ok(LeaseCommand::Configure(next)) => {
                let coordination_changed = match (config.as_deref(), next.as_deref()) {
                    (Some(previous), Some(next)) => {
                        previous.url != next.url
                            || previous.workplace_id != next.workplace_id
                            || previous.workplace_name != next.workplace_name
                    }
                    (None, None) => false,
                    _ => true,
                };
                if coordination_changed {
                    release_active(config.as_deref(), &mut active);
                    pending = None;
                }
                config = next;
            }
            Ok(LeaseCommand::Acquire {
                controller_id,
                controller_name,
                force,
            }) => {
                let Some(current) = config.as_ref() else {
                    continue;
                };
                match luxifer_application::acquire_lease(
                    &current.url,
                    &controller_id,
                    &controller_name,
                    &current.workplace_id,
                    &current.workplace_name,
                    force,
                ) {
                    Ok(reply) if reply.granted => {
                        active = reply.token.clone().map(|token| ActiveLease {
                            controller_id,
                            token,
                            usage: luxifer_application::LeaseUsage::Idle,
                        });
                        pending = None;
                        let _ = result_tx.send(LeaseWorkerResult::Acquired);
                    }
                    Ok(reply) => {
                        pending = reply.release_requested.then_some(PendingLease {
                            controller_id,
                            controller_name,
                        });
                        let _ = result_tx.send(LeaseWorkerResult::Denied(reply));
                    }
                    Err(error) => {
                        let _ = result_tx.send(LeaseWorkerResult::Lost(error.message().into()));
                    }
                }
            }
            Ok(LeaseCommand::Release) => {
                pending = None;
                release_active(config.as_deref(), &mut active);
                let _ = result_tx.send(LeaseWorkerResult::Released);
            }
            Ok(LeaseCommand::Usage(usage)) => {
                if let Some(lease) = active.as_mut() {
                    lease.usage = usage;
                    if let Some(current) = config.as_ref() {
                        let _ = luxifer_application::heartbeat_lease(
                            &current.url,
                            &lease.controller_id,
                            &current.workplace_id,
                            &lease.token,
                            lease.usage,
                        );
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let Some(current) = config.as_ref() else {
                    continue;
                };
                if active.is_none() {
                    let Some(request) = pending.as_ref() else {
                        continue;
                    };
                    match luxifer_application::acquire_lease(
                        &current.url,
                        &request.controller_id,
                        &request.controller_name,
                        &current.workplace_id,
                        &current.workplace_name,
                        false,
                    ) {
                        Ok(reply) if reply.granted => {
                            active = reply.token.map(|token| ActiveLease {
                                controller_id: request.controller_id.clone(),
                                token,
                                usage: luxifer_application::LeaseUsage::Idle,
                            });
                            pending = None;
                            let _ = result_tx.send(LeaseWorkerResult::Acquired);
                        }
                        Ok(_) => {}
                        Err(error) => {
                            pending = None;
                            let _ = result_tx.send(LeaseWorkerResult::Lost(error.message().into()));
                        }
                    }
                    continue;
                }
                let Some(lease) = active.as_ref() else {
                    continue;
                };
                match luxifer_application::heartbeat_lease(
                    &current.url,
                    &lease.controller_id,
                    &current.workplace_id,
                    &lease.token,
                    lease.usage,
                ) {
                    Ok(reply)
                        if reply.release_requested
                            && lease.usage == luxifer_application::LeaseUsage::Idle =>
                    {
                        release_active(config.as_deref(), &mut active);
                        let _ = result_tx.send(LeaseWorkerResult::ReleaseRequested);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        active = None;
                        let _ = result_tx.send(LeaseWorkerResult::Lost(error.message().into()));
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn release_active(config: Option<&CharonConfig>, active: &mut Option<ActiveLease>) {
    if let (Some(current), Some(lease)) = (config, active.as_ref()) {
        let _ = luxifer_application::release_lease(
            &current.url,
            &lease.controller_id,
            &current.workplace_id,
            &lease.token,
        );
    }
    *active = None;
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
