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

    pub fn try_from_direntry(value: DirEntry) -> Result<Option<Self>> {
        let file_type = value.file_type()?;

        if file_type.is_file() {
            let metadata: Metadata = value.metadata()?;
            Ok(Some(Self::File(FileMetadata {
                name: value.file_name().to_string_lossy().into_owned(),
                mtime: metadata.mtime(),
                size: metadata.size(),
                hash: None,
            })))
        } else if file_type.is_dir() {
            Ok(Some(Self::Dir(DirMetadata {
                name: value.file_name().to_string_lossy().into_owned(),
            })))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FileEvent {
    Create(PathBuf),
    Update(PathBuf),
    Delete(PathBuf),
}
