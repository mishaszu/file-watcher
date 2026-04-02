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
    Result, Snapshot, diff::diff_snapshots, env::Env, model::FileEvent, parser::parse_dir_blocking,
};

pub async fn controller(
    config: Env,
    mut state: Snapshot,
    sink_tx: mpsc::Sender<FileEvent>,
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
            res = async {
                match &mut scan_rx {
                    Some(rx) => rx.await.ok(),
                    None => None
                }
            }, if scan_rx.is_some() => {
                if let Some(value) = res {
                    let diff = diff_snapshots(&state, &value);
                    for event in diff {
                        if let Err(err) = sink_tx.try_send(event) {
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
                    state = value;
                }

                scan_rx = None;
            }
        }
    }
}
