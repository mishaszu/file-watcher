use std::{collections::HashMap, path::PathBuf};

use dotenv::dotenv;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    controller::controller,
    model::{Entity, FileEvent},
    parser::parse_dir_blocking,
    sink::sink_watcher,
};

mod controller;
mod diff;
mod env;
mod model;
mod parser;
mod sink;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error {0}")]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, Error>;

pub type Snapshot = HashMap<PathBuf, Entity>;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let (config, sink_kind) = env::Env::new();

    let (tx, rx) = mpsc::channel::<FileEvent>(100);

    tokio::spawn(async {
        match sink_kind {
            sink::SinkKind::Stdout(sink) => sink_watcher(sink, rx).await,
        }
    });

    let state = parse_dir_blocking(&config.root_dir)?;

    controller(config, state, tx).await?;

    Ok(())
}
