use std::{env, path::PathBuf, str::FromStr};

pub struct Env {
    pub interval_sec: u64,
    pub root_dir: PathBuf,
}

impl Env {
    pub fn new() -> Self {
        let dir = env::var("WATCH_DIR").expect("WATCH_DIR should be provided in env file");
        Env {
            interval_sec: env::var("INTERVAL_SEC")
                .expect("INTERVAL_SEC should be provided in env file")
                .parse()
                .expect("INTERVAL_SEC should be a number of seconds"),
            root_dir: PathBuf::from_str(&dir).unwrap(),
        }
    }
}
