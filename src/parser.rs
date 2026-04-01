use std::{collections::HashMap, path::PathBuf, pin::Pin};

use tokio::fs;

use crate::{Result, State, model::Entity};

pub fn read_dir(
    path: &PathBuf,
) -> Pin<Box<dyn Future<Output = Result<HashMap<PathBuf, Entity>>> + Send + '_>> {
    Box::pin(async move {
        let mut content = fs::read_dir(&path).await?;

        let mut state: State = HashMap::new();

        // TODO: check why Option my occur and if it's not early termination
        while let Ok(Some(value)) = content.next_entry().await {
            let path = value.path();

            let entry = Entity::try_from_direntry(value).await?;
            if let Some(value) = entry {
                let is_dir = value.is_dir();
                if is_dir {
                    let inner_state = read_dir(&path).await?;
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
    })
}
