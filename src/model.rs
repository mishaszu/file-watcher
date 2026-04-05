use std::{
    fs::{DirEntry, Metadata},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

use thiserror::Error;
use tokio::sync::mpsc::error::TrySendError;

use crate::Result;

#[derive(Debug)]
pub struct Watcher;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Default)]
pub enum Hash {
    #[default]
    None,
    Pending(String, u64),
    PendingNew(u64),
    Computed(String),
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub mtime: i64,
    pub size: u64,
    pub hash: Hash,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct DirMetadata {
    pub name: String,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct Item {
    pub version: u64,
    pub kind: ItemKind,
}

impl Item {
    pub fn new_file(version: u64, name: String, mtime: i64, size: u64) -> Self {
        Self {
            version,
            kind: ItemKind::File(FileMetadata {
                name,
                mtime,
                size,
                hash: Hash::None,
            }),
        }
    }

    pub fn new_file_with_update_hash(
        version: u64,
        name: String,
        mtime: i64,
        size: u64,
        next_job_id: u64,
    ) -> Self {
        Self {
            version,
            kind: ItemKind::File(FileMetadata {
                name,
                mtime,
                size,
                hash: Hash::PendingNew(next_job_id.to_owned()),
            }),
        }
    }

    pub fn update_hash(&mut self, hash: Hash) {
        if let ItemKind::File(metadata) = &mut self.kind {
            metadata.hash = hash
        }
    }

    pub fn new_dir(version: u64, name: String) -> Self {
        Self {
            version,
            kind: ItemKind::Dir(DirMetadata { name }),
        }
    }

    pub fn try_from_direntry(value: DirEntry) -> Result<Option<Self>> {
        let file_type = value.file_type()?;

        if file_type.is_file() {
            let metadata: Metadata = value.metadata()?;
            let item = Self::new_file(
                0,
                value.file_name().to_string_lossy().into_owned(),
                metadata.mtime(),
                metadata.size(),
            );
            Ok(Some(item))
        } else if file_type.is_dir() {
            Ok(Some(Self::new_dir(
                0,
                value.file_name().to_string_lossy().into_owned(),
            )))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum ItemKind {
    File(FileMetadata),
    Dir(DirMetadata),
}

impl ItemKind {
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_))
    }
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Dir(_))
    }
}

#[derive(Debug, Error, Clone)]
pub enum EventError {
    #[error("Item already exists {0}")]
    Duplicate(String),

    #[error("Item doesn't exist {0}")]
    NotFound(String),
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum Event {
    Create(PathBuf, Item),
    Update(PathBuf, Item),
    DirtyUpdate(PathBuf, Item),
    Delete(PathBuf),
}

impl Event {
    pub fn get_path(&self) -> &PathBuf {
        match self {
            Event::Create(path_buf, _)
            | Event::Update(path_buf, _)
            | Event::DirtyUpdate(path_buf, _)
            | Event::Delete(path_buf) => path_buf,
        }
    }

    pub fn compare_path(&self, path: &PathBuf) -> bool {
        match self {
            Event::Create(path_buf, _)
            | Event::Update(path_buf, _)
            | Event::DirtyUpdate(path_buf, _)
            | Event::Delete(path_buf) => path == path_buf,
        }
    }
}

pub fn try_send_to_channel<T>(
    queue_name: &str,
    res: std::result::Result<(), TrySendError<T>>,
) -> std::result::Result<(), crate::Error> {
    match res {
        Err(TrySendError::Full(_)) => {
            eprintln!("{queue_name} channel full; dropping event");
            Ok(())
        }
        Err(TrySendError::Closed(_)) => Err(crate::Error::QueueClosed(format!(
            "{queue_name} channel closed; stopping controller"
        ))),
        Ok(_) => Ok(()),
    }
}
