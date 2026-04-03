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
    hasher::{HasherIncomingMsg, HasherReadyMsg},
    helper::eprint_try_send_error,
    model::{EntityKind, Event, HashCandidateInfo, HashedInfo, SinkFileEvent},
    parser::parse_dir_blocking,
};

pub async fn controller(
    config: Env,
    mut state: Snapshot,
    sink_tx: mpsc::Sender<SinkFileEvent>,
    hash_request_tx: mpsc::Sender<HasherIncomingMsg>,
    mut hash_completion_rx: mpsc::Receiver<HasherReadyMsg>,
) -> Result<()> {
    let mut ticker = interval(Duration::from_secs(config.interval_sec));

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
            Some(HasherReadyMsg(HashedInfo{path, version, new_hash})) = hash_completion_rx.recv() => {
                println!("hash version incoming: {version}");
                if let Some(item) =  state.get_mut(&path) && item.version == version
                    && let EntityKind::File(metadata) = &mut item.kind {

                        match metadata.hash.as_ref() {
                            Some(old_hash) => {
                                let changed = old_hash != &new_hash;
                                if changed {
                                    metadata.hash = Some(new_hash);
                                    if let Err(err) = sink_tx.try_send(SinkFileEvent::Update(path.clone())) {
                                        eprint_try_send_error(err);
                                    }

                                }
                            },
                            None => {
                                metadata.hash = Some(new_hash);
                            },
                        }
                        //
                        // println!("hash changed: {}, old : {:#?}", changed, metadata.hash);

                    }
            }
            res = async {
                match &mut scan_rx {
                    Some(rx) => rx.await.ok(),
                    None => None
                }
            }, if scan_rx.is_some() => {
                if let Some(new_snapshot) = res {
                    let diff = diff_snapshots(&state, &new_snapshot);
                    queue_for_hash(&diff, Some(sink_tx.clone()), hash_request_tx.clone());
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
) {
    for event in diff {
        let sink_event: Option<SinkFileEvent> = event.try_into().ok();

        if let Some(sink_event) = sink_event
            && let Some(ref sink_tx) = sink_tx
            && let Err(err) = sink_tx.try_send(sink_event)
        {
            eprint_try_send_error(err);
        }

        match event {
            Event::Create(path, item)
            | Event::Update(path, item)
            | Event::DirtyUpdate(path, item) => {
                if item.kind.is_file() {
                    let info = HashCandidateInfo {
                        version: item.version,
                        path: path.clone(),
                    };
                    if let Err(err) = hash_request_tx.try_send(HasherIncomingMsg(info)) {
                        eprint_try_send_error(err);
                    }
                }
            }
            _ => (),
        }
    }
}
