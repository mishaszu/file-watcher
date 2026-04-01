use std::{collections::HashMap, env, path::PathBuf, str::FromStr};

use dotenv::dotenv;
use thiserror::Error;

use crate::model::Entity;
use crate::parser::read_dir;

mod model;
mod parser;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error {0}")]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, Error>;

pub type State = HashMap<PathBuf, Entity>;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let dir = env::var("WATCH_DIR").expect("WATCH_DIR should be provided in env file");

    let path = PathBuf::from_str(&dir).unwrap();

    let res = tokio::task::spawn_blocking(move || -> Result<State> {
        let state = read_dir(&path)?;
        println!("{state:#?}");

        Ok(state)
    });

    res.await.unwrap()?;

    Ok(())
}
