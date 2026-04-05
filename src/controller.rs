use std::time::Duration;

use tokio::{
    select,
    sync::{mpsc, oneshot},
    time::interval,
};

use crate::{
    Result, Snapshot,
    diff::{apply_diff, diff_snapshots},
    env::Env,
    hasher::{HashCandidateInfo, HashedInfo, HasherIncomingMsg, HasherReadyMsg},
    model::{Event, Hash, ItemKind, try_send_to_channel},
    parser::parse_dir_blocking,
    sink::SinkFileEvent,
};

pub async fn controller(
    config: Env,
    mut state: Snapshot,
    next_job_id: u64,
    sink_tx: mpsc::Sender<SinkFileEvent>,
    hash_request_tx: mpsc::Sender<HasherIncomingMsg>,
    mut hash_completion_rx: mpsc::Receiver<HasherReadyMsg>,
) -> Result<()> {
    let mut ticker = interval(Duration::from_secs(config.interval_sec));

    let mut next_job_id = next_job_id;

    let mut scan_rx: Option<oneshot::Receiver<Snapshot>> = None;

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
            Some(HasherReadyMsg(HashedInfo{path, job_id, new_hash})) = hash_completion_rx.recv() => {
                if let Some(item) =  state.get_mut(&path)
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
                        try_send_to_channel("Sink", sink_tx.try_send(SinkFileEvent::Update(path.clone())))?;
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
                    let diff = diff_snapshots(&state, &new_snapshot, &mut next_job_id);
                    queue_for_hash(&diff, Some(sink_tx.clone()), hash_request_tx.clone())?;
                    let _result = apply_diff(&mut state, diff);
                }

                scan_rx = None;
            }
        }
    }
}

pub fn queue_for_hash(
    diff: &[Event],
    sink_tx: Option<mpsc::Sender<SinkFileEvent>>,
    hash_request_tx: mpsc::Sender<HasherIncomingMsg>,
) -> Result<()> {
    for event in diff {
        let sink_event: Option<SinkFileEvent> = event.try_into().ok();

        if let Some(sink_event) = sink_event
            && let Some(ref sink_tx) = sink_tx
        {
            try_send_to_channel("Sink", sink_tx.try_send(sink_event))?;
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
