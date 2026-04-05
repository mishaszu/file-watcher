use std::{collections::HashMap, fs, os::unix::fs::MetadataExt, path::PathBuf};

use crate::{
    Error, Result, Snapshot,
    model::{DirMetadata, FileMetadata, Hash, Item, ItemKind},
};

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

pub fn parse_path(path: PathBuf, metadata: std::fs::Metadata) -> Result<(PathBuf, ItemKind)> {
    let name = path
        .file_name()
        .ok_or_else(|| Error::Path(path.to_owned()))?
        .to_string_lossy()
        .into_owned();
    if metadata.is_file() {
        Ok((
            path.to_owned(),
            ItemKind::File(FileMetadata {
                name,
                mtime: metadata.mtime(),
                size: metadata.size(),
                hash: Hash::None,
            }),
        ))
    } else if metadata.is_dir() {
        Ok((path.to_owned(), ItemKind::Dir(DirMetadata { name })))
    } else {
        Err(Error::Path(path.to_owned()))
    }
}
