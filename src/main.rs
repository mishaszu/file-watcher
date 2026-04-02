use std::{collections::HashMap, path::PathBuf};

use dotenv::dotenv;
use thiserror::Error;

use crate::{controller::controller, model::Entity, parser::parse_dir_blocking};

mod controller;
mod diff;
mod env;
mod model;
mod parser;

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
    let config = env::Env::new();

    let state = parse_dir_blocking(&config.root_dir)?;

    controller(config, state).await?;

    Ok(())
}
