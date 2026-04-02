use std::time::Duration;

use tokio::{
    select,
    sync::{
        mpsc::{self, error::TrySendError},
        oneshot,
    },
    time::interval,
};

use crate::{
    Result, Snapshot,
    diff::diff_snapshots,
    env::Env,
    hasher::{HasherIncomingMsg, HasherReadyMsg},
    model::{EntityKind, FileEvent, ItemKind},
    parser::parse_dir_blocking,
};

pub async fn controller(
    config: Env,
    mut state: Snapshot,
    sink_tx: mpsc::Sender<FileEvent>,
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
            Some(HasherReadyMsg(version, path, hash)) = hash_completion_rx.recv() => {
                if let Some(item) =  state.get_mut(&path) && item.version == version
                    && let EntityKind::File(metadata) = &mut item.kind {
                        metadata.hash = Some(hash);
                    }
            }
            res = async {
                match &mut scan_rx {
                    Some(rx) => rx.await.ok(),
                    None => None
                }
            }, if scan_rx.is_some() => {
                if let Some(mut value) = res {
                    let diff = diff_snapshots(&state, &mut value);
                    for (version, item_kind, event) in diff {
                        if let Err(err) = sink_tx.try_send(event.clone()) {
                            match err {
                                TrySendError::Full(_)=> {
                                    eprintln!("controller: sink channel closed; stopping controller");
                                },
                                TrySendError::Closed(_)=> {
                                    eprintln!("controller: sink channel full; dropping event");
                                }
                            }
                        }

                        match (item_kind, event) {
                            (ItemKind::File, FileEvent::Create(path)) |
                            (ItemKind::File, FileEvent::Update(path))  => {

                            if let Err(err) = hash_request_tx.try_send(HasherIncomingMsg(version, path)) {
                            match err {
                                TrySendError::Full(_)=> {
                                    eprintln!("controller: sink channel closed; stopping controller");
                                },
                                TrySendError::Closed(_)=> {
                                    eprintln!("controller: sink channel full; dropping event");
                                }
                            }
                        }
                            }
                            _ => ()
                    }
                    }
                    state = value;
                }

                scan_rx = None;
            }
        }
    }
}
