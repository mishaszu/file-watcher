use crate::{
    Snapshot,
    model::{EntityKind, FileEvent, ItemKind},
};

pub fn diff_snapshots(old: &Snapshot, new: &mut Snapshot) -> Vec<(u64, ItemKind, FileEvent)> {
    let mut events = Vec::new();

    for (path, new_item) in new.iter_mut() {
        if let Some(old_item) = old.get(path) {
            match (&new_item.kind, &old_item.kind) {
                (EntityKind::File(new_file_metadata), EntityKind::File(old_file_metadata)) => {
                    let new_version = old_item.version + 1;
                    if new_file_metadata.size != old_file_metadata.size {
                        events.push((
                            new_version,
                            ItemKind::File,
                            FileEvent::Update(path.to_owned()),
                        ));
                        new_item.version = new_version;
                    } else if new_file_metadata.mtime != old_file_metadata.mtime {
                        match (
                            new_file_metadata.hash.as_ref(),
                            old_file_metadata.hash.as_ref(),
                        ) {
                            (Some(new_hash), Some(old_hash)) if new_hash == old_hash => (),
                            // Naive approach. With async hash calculating would need another
                            // approach to sync hashes before diff to make proper comparison
                            _ => {
                                events.push((
                                    new_version,
                                    ItemKind::File,
                                    FileEvent::Update(path.to_owned()),
                                ));
                                new_item.version = new_version;
                            }
                        }
                    }
                }
                (EntityKind::File(_), EntityKind::Dir(_)) => {
                    events.push((0, ItemKind::Dir, FileEvent::Delete(path.to_owned())));
                    events.push((0, ItemKind::File, FileEvent::Create(path.to_owned())));
                }
                (EntityKind::Dir(_), EntityKind::File(_)) => {
                    events.push((0, ItemKind::File, FileEvent::Delete(path.to_owned())));
                    events.push((0, ItemKind::Dir, FileEvent::Create(path.to_owned())));
                }
                _ => (),
            }
        } else {
            events.push((
                0,
                ItemKind::from(new_item),
                FileEvent::Create(path.to_owned()),
            ));
        }
    }

    for (path, item) in old.iter() {
        if !new.contains_key(path) {
            events.push((0, ItemKind::from(item), FileEvent::Delete(path.to_owned())));
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
        model::{DirMetadata, EntityKind, FileEvent, FileMetadata, Item, ItemKind},
    };

    fn create_snapshot_1() -> Snapshot {
        let mut snapshot = HashMap::new();

        snapshot.insert(
            PathBuf::from_str("/test1.txt").unwrap(),
            Item::new_file("test1".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_dir("test".to_string()),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test2.txt").unwrap(),
            Item::new_file("test2".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test3.txt").unwrap(),
            Item::new_file("test3".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test4.txt").unwrap(),
            Item::new_file("test4".to_string(), 10, 1000),
        );

        snapshot
    }

    fn create_snapshot_2() -> Snapshot {
        let mut snapshot = HashMap::new();

        snapshot.insert(
            PathBuf::from_str("/test1.txt").unwrap(),
            Item::new_file("test1".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_dir("test".to_string()),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test2.txt").unwrap(),
            Item::new_file("test2".to_string(), 11, 1030),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test3.txt").unwrap(),
            Item::new_file("test3".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test5.txt").unwrap(),
            Item::new_file("test5".to_string(), 10, 1000),
        );

        snapshot
    }

    #[test]
    fn diff_snapshots_1() {
        let snapshot1 = create_snapshot_1();
        let mut snapshot2 = create_snapshot_2();

        let mut diff = diff_snapshots(&snapshot1, &mut snapshot2);

        diff.sort();

        let mut expected = vec![
            (
                0,
                ItemKind::File,
                FileEvent::Create(PathBuf::from_str("/test5.txt").unwrap()),
            ),
            (
                0,
                ItemKind::File,
                FileEvent::Delete(PathBuf::from_str("/test4.txt").unwrap()),
            ),
            (
                0,
                ItemKind::File,
                FileEvent::Update(PathBuf::from_str("/test/test2.txt").unwrap()),
            ),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_snapshots_2() {
        let snapshot1 = create_snapshot_2();
        let mut snapshot2 = create_snapshot_1();

        let mut diff = diff_snapshots(&snapshot1, &mut snapshot2);

        diff.sort();

        let mut expected = vec![
            (
                0,
                ItemKind::File,
                FileEvent::Delete(PathBuf::from_str("/test5.txt").unwrap()),
            ),
            (
                0,
                ItemKind::File,
                FileEvent::Create(PathBuf::from_str("/test4.txt").unwrap()),
            ),
            (
                0,
                ItemKind::File,
                FileEvent::Update(PathBuf::from_str("/test/test2.txt").unwrap()),
            ),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_for_type_change() {
        let mut snapshot1: Snapshot = HashMap::new();
        snapshot1.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_dir("test".to_string()),
        );
        let mut snapshot2 = HashMap::new();
        snapshot2.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_file("text".to_string(), 100, 1000),
        );

        let mut diff = diff_snapshots(&snapshot1, &mut snapshot2);

        diff.sort();

        let mut expected = vec![
            (
                0,
                ItemKind::Dir,
                FileEvent::Delete(PathBuf::from_str("/test").unwrap()),
            ),
            (
                0,
                ItemKind::File,
                FileEvent::Create(PathBuf::from_str("/test").unwrap()),
            ),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }
}
