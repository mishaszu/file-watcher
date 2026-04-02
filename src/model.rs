use std::{
    fs::{DirEntry, Metadata},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

use crate::Result;

#[derive(Debug)]
pub struct FileMetadata {
    pub name: String,
    pub mtime: i64,
    pub size: u64,
    pub hash: Option<String>,
}

#[derive(Debug)]
pub struct Watcher;

#[derive(Debug)]
pub struct DirMetadata {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    Dir,
    File,
}

impl<'a> From<&'a mut Item> for ItemKind {
    fn from(value: &'a mut Item) -> Self {
        if value.kind.is_file() {
            Self::File
        } else {
            Self::Dir
        }
    }
}

impl<'a> From<&'a Item> for ItemKind {
    fn from(value: &'a Item) -> Self {
        if value.kind.is_file() {
            Self::File
        } else {
            Self::Dir
        }
    }
}

#[derive(Debug)]
pub struct Item {
    pub version: u64,
    pub kind: EntityKind,
}

impl Item {
    pub fn new_file(name: String, mtime: i64, size: u64) -> Self {
        Self {
            version: 0,
            kind: EntityKind::File(FileMetadata {
                name,
                mtime,
                size,
                hash: None,
            }),
        }
    }

    pub fn new_dir(name: String) -> Self {
        Self {
            version: 0,
            kind: EntityKind::Dir(DirMetadata { name }),
        }
    }

    pub fn try_from_direntry(value: DirEntry) -> Result<Option<Self>> {
        let file_type = value.file_type()?;

        if file_type.is_file() {
            let metadata: Metadata = value.metadata()?;
            Ok(Some(Self::new_file(
                value.file_name().to_string_lossy().into_owned(),
                metadata.mtime(),
                metadata.size(),
            )))
        } else if file_type.is_dir() {
            Ok(Some(Self::new_dir(
                value.file_name().to_string_lossy().into_owned(),
            )))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
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
pub enum FileEvent {
    Create(PathBuf),
    Update(PathBuf),
    Delete(PathBuf),
}
