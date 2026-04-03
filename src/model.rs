use std::{
    fs::{DirEntry, Metadata},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

use thiserror::Error;

use crate::Result;

#[derive(Debug)]
pub struct Watcher;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub mtime: i64,
    pub size: u64,
    pub hash: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct DirMetadata {
    pub name: String,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct Item {
    pub version: u64,
    pub kind: EntityKind,
}

impl Item {
    pub fn new_file(version: u64, name: String, mtime: i64, size: u64) -> Self {
        Self {
            version,
            kind: EntityKind::File(FileMetadata {
                name,
                mtime,
                size,
                hash: None,
            }),
        }
    }

    pub fn new_dir(version: u64, name: String) -> Self {
        Self {
            version,
            kind: EntityKind::Dir(DirMetadata { name }),
        }
    }

    pub fn try_from_direntry(value: DirEntry) -> Result<Option<Self>> {
        let file_type = value.file_type()?;

        if file_type.is_file() {
            let metadata: Metadata = value.metadata()?;
            Ok(Some(Self::new_file(
                0,
                value.file_name().to_string_lossy().into_owned(),
                metadata.mtime(),
                metadata.size(),
            )))
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
pub enum EntityKind {
    File(FileMetadata),
    Dir(DirMetadata),
}

impl EntityKind {
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File(_))
    }
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Dir(_))
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct HashCandidateInfo {
    pub version: u64,
    pub path: PathBuf,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct HashedInfo {
    pub version: u64,
    pub path: PathBuf,
    pub new_hash: String,
}

pub type EventResult<T> = std::result::Result<T, EventError>;

#[derive(Debug, Error)]
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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum SinkFileEvent {
    Create(PathBuf),
    Update(PathBuf),
    Delete(PathBuf),
}

impl TryFrom<&Event> for SinkFileEvent {
    type Error = Box<dyn std::error::Error>;

    fn try_from(ev: &Event) -> std::result::Result<Self, Self::Error> {
        match ev {
            Event::Create(path, _) => Ok(Self::Create(path.to_owned())),
            Event::Update(path, _) => Ok(Self::Update(path.to_owned())),
            Event::DirtyUpdate(_, _) => {
                Err(std::io::Error::other("Dirty update can't be converted to sink update").into())
            }
            Event::Delete(path) => Ok(Self::Delete(path.to_owned())),
        }
    }
}
