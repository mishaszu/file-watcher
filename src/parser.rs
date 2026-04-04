use std::{collections::HashMap, fs, path::PathBuf};

use crate::{Result, Snapshot, model::Item};

pub fn parse_dir_blocking(path: &PathBuf) -> Result<Snapshot> {
    let content = fs::read_dir(path)?;

    let mut state: Snapshot = HashMap::new();

    for item in content {
        let item = item?;
        let path = item.path();

        let entry = Item::try_from_direntry(item)?;
        if let Some(value) = entry {
            let is_dir = value.kind.is_dir();
            if is_dir {
                let inner_state = parse_dir_blocking(&path)?;
                state.insert(path, value);
                inner_state.into_iter().for_each(|(path, entity)| {
                    state.insert(path, entity);
                });
            } else {
                state.insert(path, value);
            }
        }
    }

    Ok(state)
}
