use std::{collections::HashMap, path::PathBuf, time::Duration};

use dotenv::dotenv;
use notify::{Event, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::{select, sync::mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    controller::{ControllerDeps, ControllerState, controller, queue_for_hash},
    diff::diff_snapshots,
    hasher::{HasherIncomingMsg, HasherReadyMsg, hash_worker},
    model::{Item, try_send_to_channel},
    parser::parse_dir_blocking,
    sink::{SinkFileEvent, sink_watcher},
};

mod controller;
mod diff;
mod env;
mod hasher;
mod model;
mod parser;
mod sink;
mod watcher;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    #[error("notify error {0}")]
    Notify(#[from] notify::Error),

    #[error("queue channel was closed: {0}")]
    QueueClosed(String),

    #[error("semaphore was closed, can't issue new permits")]
    SemaphoreClosed,

    #[error("wrong path for an item {0}")]
    Path(PathBuf),
}

type Result<T> = std::result::Result<T, Error>;

pub type Snapshot = HashMap<PathBuf, Item>;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let (config, sink_kind) = env::Env::new();
    let token = CancellationToken::new();

    let (file_event_tx, file_event_rx) = mpsc::channel::<SinkFileEvent>(100);
    let (hash_request_tx, hash_request_rx) = mpsc::channel::<HasherIncomingMsg>(100);
    let (hash_completion_tx, hash_completion_rx) = mpsc::channel::<HasherReadyMsg>(100);

    let (watcher_tx, watcher_rx) = mpsc::channel::<notify::Result<Event>>(100);

    let (error_tx, mut error_rx) = mpsc::channel::<String>(100);

    let controller_deps = ControllerDeps {
        config: config.clone(),
        sink_tx: file_event_tx,
        hash_request_tx: hash_request_tx.clone(),
        hash_completion_rx,
        watcher_rx,
        error_tx: error_tx.clone(),
    };

    let watcher_token = token.clone();
    let watcher_error_tx = error_tx.clone();
    let mut watcher = notify::recommended_watcher(move |res| {
        // non-blocking but might miss events
        match try_send_to_channel("Watcher", &watcher_error_tx, watcher_tx.try_send(res)) {
            Ok(_) => (),
            Err(err) => {
                eprintln!("{err}");
                watcher_token.cancel();
            }
        }
    })
    .map_err(Error::Notify)?;

    watcher.watch(&config.root_dir.clone(), RecursiveMode::Recursive)?;

    // hasher task
    let hasher_token = token.clone();
    tokio::spawn(async move {
        if let Err(err) = hash_worker(hash_request_rx, hash_completion_tx).await {
            eprintln!("hash worker failed: {err}");
            hasher_token.cancel();
        }
    });

    // sink task
    tokio::spawn(async {
        match sink_kind {
            sink::SinkKind::Stdout(sink) => sink_watcher(sink, file_event_rx).await,
        }
    });

    // error sink task
    tokio::spawn(async move {
        while let Some(event) = error_rx.recv().await {
            eprintln!("{event}");
        }
    });

    // init state
    let mut next_job_id = 0;
    println!("FileWatcher: building root directory snapshot");
    let state = parse_dir_blocking(&config.root_dir)?;
    let diff = diff_snapshots(&HashMap::new(), &state, &mut next_job_id);
    let controller_state = ControllerState {
        next_job_id,
        snapshot: state,
    };

    let initial_hasher_token = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Err(err) = queue_for_hash(&diff, None, &hash_request_tx, &error_tx) {
            eprintln!("FileWatcher: failed to queue initial hash requests: {err}");
            initial_hasher_token.cancel();
        }
    });

    println!("FileWatcher: starting watcher");

    select! {
        _ = token.cancelled() => (),
        res = controller(
                controller_state,
                controller_deps
            ) => {
            if let Err(err) = res {
                eprintln!("{err:#?}");
                token.cancel();
            }
        }
    }

    Ok(())
}
