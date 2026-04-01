use std::{fs::Metadata, os::unix::fs::MetadataExt, path::PathBuf};

use tokio::fs::DirEntry;

use crate::Result;

#[derive(Debug)]
pub struct FileMetadata {
    name: String,
    mtime: i64,
    size: u64,
    hash: Option<String>,
}

#[derive(Debug)]
pub struct Watcher;

#[derive(Debug)]
pub struct DirMetadata {
    name: String,
    watcher: Option<Watcher>,
}

#[derive(Debug)]
pub enum Entity {
    File(FileMetadata),
    Dir(DirMetadata),
}

impl Entity {
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_))
    }
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Dir(_))
    }

    pub async fn try_from_direntry(value: DirEntry) -> Result<Option<Self>> {
        let file_type = value.file_type().await?;

        if file_type.is_file() {
            let metadata: Metadata = value.metadata().await?;
            Ok(Some(Self::File(FileMetadata {
                name: value.file_name().to_string_lossy().into_owned(),
                mtime: metadata.mtime(),
                size: metadata.size(),
                hash: None,
            })))
        } else if file_type.is_dir() {
            Ok(Some(Self::Dir(DirMetadata {
                name: value.file_name().to_string_lossy().into_owned(),
                watcher: None,
            })))
        } else {
            Ok(None)
        }
    }
}

pub enum FileEvent {
    Report(PathBuf),
    Create(PathBuf),
    Update(PathBuf),
    Delete(PathBuf),
}
