use std::{collections::HashMap, path::PathBuf, time::Duration};

use dotenv::dotenv;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    controller::{controller, queue_for_hash},
    diff::diff_snapshots,
    hasher::{HasherIncomingMsg, HasherReadyMsg, hash_worker},
    model::Item,
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

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    #[error("queue channel was closed: {0}")]
    QueueClosed(String),

    #[error("semaphore was closed, can't issue new permits")]
    SemaphoreClosed,
}

type Result<T> = std::result::Result<T, Error>;

pub type Snapshot = HashMap<PathBuf, Item>;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let (config, sink_kind) = env::Env::new();

    let (file_event_tx, file_event_rx) = mpsc::channel::<SinkFileEvent>(100);
    let (hash_request_tx, hash_request_rx) = mpsc::channel::<HasherIncomingMsg>(100);
    let (hash_completion_tx, hash_completion_rx) = mpsc::channel::<HasherReadyMsg>(100);

    tokio::spawn(async {
        if let Err(err) = hash_worker(hash_request_rx, hash_completion_tx).await {
            // TODO: it should trigger shotdown
            eprintln!("hash worker failed: {err}");
        }
    });

    tokio::spawn(async {
        match sink_kind {
            sink::SinkKind::Stdout(sink) => sink_watcher(sink, file_event_rx).await,
        }
    });

    let mut next_job_id = 0;
    println!("FileWatcher: building root directory snapshot");
    let state = parse_dir_blocking(&config.root_dir)?;
    let diff = diff_snapshots(&HashMap::new(), &state, &mut next_job_id);

    let hash_request_tx2 = hash_request_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Err(err) = queue_for_hash(&diff, None, hash_request_tx2) {
            // TODO: it should trigger shotdown
            eprintln!("FileWatcher: failed to queue initial hash requests: {err}");
        }
    });

    println!("FileWatcher: starting watcher");
    controller(
        config,
        state,
        next_job_id,
        file_event_tx,
        hash_request_tx,
        hash_completion_rx,
    )
    .await?;

    Ok(())
}
