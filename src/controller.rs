use std::time::Duration;

use tokio::{
    select,
    sync::{mpsc, oneshot},
    time::{interval, sleep},
};

use crate::{
    Result, Snapshot,
    diff::{apply_diff, diff_snapshots, find_n_diff_item},
    env::Env,
    hasher::{HashCandidateInfo, HashedInfo, HasherIncomingMsg, HasherReadyMsg},
    model::{Event, Hash, ItemKind, try_send_to_channel},
    parser::{parse_dir_blocking, parse_path},
    sink::SinkFileEvent,
    watcher::{OperationNeeded, WatcherMsg, accept_event},
};

pub struct ControllerDeps {
    pub config: Env,
    pub sink_tx: mpsc::Sender<SinkFileEvent>,
    pub hash_request_tx: mpsc::Sender<HasherIncomingMsg>,
    pub hash_completion_rx: mpsc::Receiver<HasherReadyMsg>,
    pub watcher_rx: mpsc::Receiver<notify::Result<notify::Event>>,
    pub error_tx: mpsc::Sender<String>,
}

pub struct ControllerState {
    pub snapshot: Snapshot,
    pub next_job_id: u64,
}

pub async fn controller(
    ControllerState {
        mut snapshot,
        next_job_id,
    }: ControllerState,
    ControllerDeps {
        config,
        sink_tx,
        hash_request_tx,
        mut hash_completion_rx,
        mut watcher_rx,
        error_tx,
    }: ControllerDeps,
) -> Result<()> {
    let mut ticker = interval(Duration::from_secs(config.interval_sec));

    let mut next_job_id = next_job_id;

    let mut scan_rx: Option<oneshot::Receiver<Snapshot>> = None;

    let (single_event_tx, mut single_event_rx) = mpsc::channel::<WatcherMsg>(1000);

    loop {
        select! {
            _ = ticker.tick() => {
                if scan_rx.is_none() {
                    let (tx, rx) = oneshot::channel();
                    scan_rx = Some(rx);
                    let path = config.root_dir.clone();

                    tokio::task::spawn_blocking(move || -> Result<()> {
                        let snapshot = parse_dir_blocking(&path)?;
                        tx.send(snapshot).unwrap();
                        Ok(())
                    });
                }
            }
            Some(event) = single_event_rx.recv() => {
                match event {
                    WatcherMsg::ItemChange((path, item_kind)) => {
                        let diff = find_n_diff_item(&snapshot, (path, item_kind), &mut next_job_id) ;
                        queue_for_hash(&diff, Some(&sink_tx), &hash_request_tx, &error_tx)?;
                        let results = apply_diff(&mut snapshot, diff);

                        for result in results {
                            let _ = error_tx.try_send(result.to_string());
                        }

                    },
                    WatcherMsg::Delete(path) => {

                        snapshot.remove(&path);
                        try_send_to_channel("Sink", &error_tx, sink_tx.try_send(SinkFileEvent::Delete(path)))?;
                    },
                }
            }
            Some(event) = watcher_rx.recv() => {
                if let Ok(event) = event
                    && let Some(operation) = accept_event(&event) {
                        let tx = single_event_tx.clone();
                        let error_tx = error_tx.clone();
                        tokio::task::spawn(async move {
                            match operation {
                                OperationNeeded::Scan(path) => {
                                    // small debounce to filter out temp dirs
                                    sleep(Duration::from_millis(100)).await;
                                    if let Ok(metadata) = std::fs::metadata(&path) {
                                        if let Ok(event) = parse_path(path, metadata) {
                                            if let Err(err) = tx.send(WatcherMsg::ItemChange(event)).await {
                                                let _ = error_tx.try_send(err.to_string());
                                            };
                                        } else {
                                            let _ = error_tx.try_send("Can't read file which change was reported by watcher".to_string());
                                        }
                                    }
                                }
                                OperationNeeded::Delete(path) => {
                                    // it will include temp dir
                                    if let Err(err) = tx.send(WatcherMsg::Delete(path)).await {
                                        let _ = error_tx.try_send(format!("failed to forward delete event to controller: {err}"));
                                    }
                                }
                            }
                        });
                    }
            }
            Some(HasherReadyMsg(HashedInfo{path, job_id, new_hash})) = hash_completion_rx.recv() => {
                if let Some(item) =  snapshot.get_mut(&path)
                    && let ItemKind::File(metadata) = &mut item.kind {
                    let should_send_update = match &mut metadata.hash {
                        Hash::PendingNew(assigned_job_id) if *assigned_job_id == job_id => {
                            metadata.hash = Hash::Computed(new_hash);
                            false
                        },
                        Hash::Pending(old_hash, assigned_job_id) if *assigned_job_id == job_id && *old_hash != new_hash => {
                            metadata.hash = Hash::Computed(new_hash);
                            true
                        },
                        Hash::None  => {
                            metadata.hash = Hash::Computed(new_hash);
                            false
                        }
                        _ => {
                            false
                        }
                    };
                    if should_send_update {
                        try_send_to_channel("Sink", &error_tx, sink_tx.try_send(SinkFileEvent::Update(path.clone())))?;
                    }
                }
            }
            res = async {
                match &mut scan_rx {
                    Some(rx) => rx.await.ok(),
                    None => None
                }
            }, if scan_rx.is_some() => {
                if let Some(new_snapshot) = res {
                    let diff = diff_snapshots(&snapshot, &new_snapshot, &mut next_job_id);
                    queue_for_hash(&diff, Some(&sink_tx), &hash_request_tx, &error_tx)?;
                    let results = apply_diff(&mut snapshot, diff);

                    for result in results {
                        let _ = error_tx.try_send(result.to_string());
                    }
                }

                scan_rx = None;
            }
        }
    }
}

pub fn queue_for_hash(
    diff: &[Event],
    sink_tx: Option<&mpsc::Sender<SinkFileEvent>>,
    hash_request_tx: &mpsc::Sender<HasherIncomingMsg>,
    error_tx: &mpsc::Sender<String>,
) -> Result<()> {
    for event in diff {
        let sink_event: Option<SinkFileEvent> = event.try_into().ok();

        if let Some(sink_event) = sink_event
            && let Some(sink_tx) = sink_tx
        {
            try_send_to_channel("Sink", error_tx, sink_tx.try_send(sink_event))?;
        }

        match event {
            Event::Create(path, item)
            | Event::Update(path, item)
            | Event::DirtyUpdate(path, item) => {
                if let ItemKind::File(metadata) = &item.kind {
                    match metadata.hash {
                        Hash::PendingNew(job_id) | Hash::Pending(_, job_id) => {
                            let info = HashCandidateInfo {
                                job_id,
                                path: path.clone(),
                            };
                            try_send_to_channel(
                                "Hasher",
                                error_tx,
                                hash_request_tx.try_send(HasherIncomingMsg(info)),
                            )?;
                        }
                        _ => (),
                    }
                }
            }
            _ => (),
        }
    }
    Ok(())
}
