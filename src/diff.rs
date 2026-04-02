use crate::{
    Snapshot,
    model::{Entity, FileEvent},
};

pub fn diff_snapshots(old: &Snapshot, new: &Snapshot) -> Vec<FileEvent> {
    let mut events = Vec::new();

    for (path, entity) in new.iter() {
        if let Some(old_entity) = old.get(path) {
            match (entity, old_entity) {
                (Entity::File(new_file_metadata), Entity::File(old_file_metadata)) => {
                    if new_file_metadata.size != old_file_metadata.size {
                        events.push(FileEvent::Update(path.to_owned()));
                    } else if new_file_metadata.mtime != old_file_metadata.mtime {
                        match (
                            new_file_metadata.hash.as_ref(),
                            old_file_metadata.hash.as_ref(),
                        ) {
                            (Some(new_hash), Some(old_hash)) if new_hash == old_hash => (),
                            // Naive approach. With async hash calculating would need another
                            // approach to sync hashes before diff to make proper comparison
                            _ => events.push(FileEvent::Update(path.to_owned())),
                        }
                    }
                }
                (Entity::File(_), Entity::Dir(_)) | (Entity::Dir(_), Entity::File(_)) => {
                    events.push(FileEvent::Delete(path.to_owned()));
                    events.push(FileEvent::Create(path.to_owned()));
                }
                _ => (),
            }
        } else {
            events.push(FileEvent::Create(path.to_owned()));
        }
    }

    for (path, _) in old.iter() {
        if !new.contains_key(path) {
            events.push(FileEvent::Delete(path.to_owned()))
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, str::FromStr};

    use crate::{
        Snapshot,
        diff::diff_snapshots,
        model::{DirMetadata, Entity, FileEvent, FileMetadata},
    };

    fn create_snapshot_1() -> Snapshot {
        let mut snapshot = HashMap::new();

        snapshot.insert(
            PathBuf::from_str("/test1.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test1".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test").unwrap(),
            Entity::Dir(DirMetadata {
                name: "test".to_string(),
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test2.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test2".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test3.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test3".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test4.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test4".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot
    }

    fn create_snapshot_2() -> Snapshot {
        let mut snapshot = HashMap::new();

        snapshot.insert(
            PathBuf::from_str("/test1.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test1".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test").unwrap(),
            Entity::Dir(DirMetadata {
                name: "test".to_string(),
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test2.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test2".to_string(),
                mtime: 11,
                size: 1030,
                hash: None,
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test3.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test3".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot.insert(
            PathBuf::from_str("/test5.txt").unwrap(),
            Entity::File(FileMetadata {
                name: "test5".to_string(),
                mtime: 10,
                size: 1000,
                hash: None,
            }),
        );

        snapshot
    }

    #[test]
    fn diff_snapshots_1() {
        let snapshot1 = create_snapshot_1();
        let snapshot2 = create_snapshot_2();

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut expected = vec![
            FileEvent::Create(PathBuf::from_str("/test5.txt").unwrap()),
            FileEvent::Delete(PathBuf::from_str("/test4.txt").unwrap()),
            FileEvent::Update(PathBuf::from_str("/test/test2.txt").unwrap()),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_snapshots_2() {
        let snapshot1 = create_snapshot_2();
        let snapshot2 = create_snapshot_1();

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut expected = vec![
            FileEvent::Delete(PathBuf::from_str("/test5.txt").unwrap()),
            FileEvent::Create(PathBuf::from_str("/test4.txt").unwrap()),
            FileEvent::Update(PathBuf::from_str("/test/test2.txt").unwrap()),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_for_type_change() {
        let mut snapshot1: Snapshot = HashMap::new();
        snapshot1.insert(
            PathBuf::from_str("/test").unwrap(),
            Entity::Dir(DirMetadata {
                name: "test".to_string(),
            }),
        );
        let mut snapshot2 = HashMap::new();
        snapshot2.insert(
            PathBuf::from_str("/test").unwrap(),
            Entity::File(FileMetadata {
                name: "text".to_string(),
                mtime: 100,
                size: 1000,
                hash: None,
            }),
        );

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut expected = vec![
            FileEvent::Delete(PathBuf::from_str("/test").unwrap()),
            FileEvent::Create(PathBuf::from_str("/test").unwrap()),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }
}
