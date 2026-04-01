use std::{collections::HashMap, fs, path::PathBuf};

use crate::{Result, State, model::Entity};

pub fn read_dir(path: &PathBuf) -> Result<HashMap<PathBuf, Entity>> {
    let mut content = fs::read_dir(path)?;

    let mut state: State = HashMap::new();

    // TODO: check why Option my occur and if it's not early termination
    while let Some(Ok(value)) = content.next() {
        let path = value.path();

        let entry = Entity::try_from_direntry(value)?;
        if let Some(value) = entry {
            let is_dir = value.is_dir();
            if is_dir {
                let inner_state = read_dir(&path)?;
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
