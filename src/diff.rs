use crate::{
    Snapshot,
    model::{Event, EventError, Item, ItemKind},
};

pub fn diff_snapshots(old: &Snapshot, new: &Snapshot) -> Vec<Event> {
    let mut events = Vec::new();

    for (path, new_item) in new.iter() {
        if let Some(old_item) = old.get(path) {
            let new_version = old_item.version + 1;
            match (&new_item.kind, &old_item.kind) {
                (ItemKind::File(new_file_metadata), ItemKind::File(old_file_metadata)) => {
                    if new_file_metadata.size != old_file_metadata.size {
                        // size changed, need to calculate new hash but doesn't need hash
                        // comparison
                        events.push(Event::Update(
                            path.to_owned(),
                            Item::new_file(
                                new_version,
                                new_file_metadata.name.clone(),
                                new_file_metadata.mtime,
                                new_file_metadata.size,
                            ),
                        ));
                    } else if new_file_metadata.mtime != old_file_metadata.mtime {
                        match (
                            new_file_metadata.hash.as_ref(),
                            old_file_metadata.hash.as_ref(),
                        ) {
                            (_, Some(_)) => {
                                // if old hash was present new hash have to be generated to compare
                                // for change
                                let mut item = new_item.clone();
                                item.version = new_version;

                                if let ItemKind::File(metadata) = &mut item.kind {
                                    metadata.hash = old_file_metadata.hash.clone();
                                }

                                events.push(Event::DirtyUpdate(path.to_owned(), item));
                            }
                            // Naive approach. Always update if no hash and mtime change.
                            // With async hash calculating would need another
                            // approach to sync hashes before diff to make proper comparison
                            _ => {
                                events.push(Event::Update(
                                    path.to_owned(),
                                    Item::new_file(
                                        new_version,
                                        new_file_metadata.name.clone(),
                                        new_file_metadata.mtime,
                                        new_file_metadata.size,
                                    ),
                                ));
                            }
                        }
                    }
                }
                (ItemKind::File(metadata), ItemKind::Dir(_)) => {
                    events.push(Event::Delete(path.to_owned()));
                    events.push(Event::Create(
                        path.to_owned(),
                        Item::new_file(
                            new_version,
                            metadata.name.clone(),
                            metadata.mtime,
                            metadata.size,
                        ),
                    ));
                }
                (ItemKind::Dir(metadata), ItemKind::File(_)) => {
                    events.push(Event::Delete(path.to_owned()));
                    events.push(Event::Create(
                        path.to_owned(),
                        Item::new_dir(new_version, metadata.name.clone()),
                    ));
                }
                _ => (),
            }
        } else {
            events.push(Event::Create(path.to_owned(), new_item.clone()));
        }
    }

    for (path, _) in old.iter() {
        if !new.contains_key(path) {
            events.push(Event::Delete(path.to_owned()));
        }
    }

    events
}

pub fn apply_diff(snapshot: &mut Snapshot, diff: Vec<Event>) -> Vec<EventError> {
    let mut res: Vec<EventError> = Vec::new();

    for event in diff {
        match event {
            Event::Create(path, item) => {
                let item_exist = snapshot.contains_key(&path);
                if item_exist {
                    res.push(EventError::Duplicate(path.to_string_lossy().into_owned()));
                } else {
                    snapshot.insert(path, item);
                }
            }
            Event::Update(path, item) | Event::DirtyUpdate(path, item) => {
                if let Some(old_item) = snapshot.get_mut(&path) {
                    if old_item.version < item.version {
                        *old_item = item;
                    }
                } else {
                    res.push(EventError::NotFound(path.to_string_lossy().into_owned()));
                }
            }
            Event::Delete(path) => {
                snapshot.remove(&path);
            }
        }
    }

    res
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, str::FromStr};

    use crate::{
        Snapshot,
        diff::{apply_diff, diff_snapshots},
        model::{Event, Item, ItemKind},
    };

    fn create_snapshot_1() -> Snapshot {
        let mut snapshot = HashMap::new();

        let path = PathBuf::from_str("/test1.txt").unwrap();
        snapshot.insert(
            path.clone(),
            Item::new_file(0, "test1".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_dir(0, "test".to_string()),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test2.txt").unwrap(),
            Item::new_file(0, "test2".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test3.txt").unwrap(),
            Item::new_file(0, "test3".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test4.txt").unwrap(),
            Item::new_file(0, "test4".to_string(), 10, 1000),
        );

        snapshot
    }

    fn create_snapshot_2() -> Snapshot {
        let mut snapshot = HashMap::new();

        snapshot.insert(
            PathBuf::from_str("/test1.txt").unwrap(),
            Item::new_file(0, "test1".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_dir(0, "test".to_string()),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test2.txt").unwrap(),
            Item::new_file(0, "test2".to_string(), 11, 1030),
        );

        snapshot.insert(
            PathBuf::from_str("/test/test3.txt").unwrap(),
            Item::new_file(0, "test3".to_string(), 10, 1000),
        );

        snapshot.insert(
            PathBuf::from_str("/test5.txt").unwrap(),
            Item::new_file(0, "test5".to_string(), 10, 1000),
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
            Event::Create(
                PathBuf::from_str("/test5.txt").unwrap(),
                Item::new_file(0, "test5".to_string(), 10, 1000),
            ),
            Event::Delete(PathBuf::from_str("/test4.txt").unwrap()),
            Event::Update(
                PathBuf::from_str("/test/test2.txt").unwrap(),
                Item::new_file(1, "test2".to_string(), 11, 1030),
            ),
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
            Event::Delete(PathBuf::from_str("/test5.txt").unwrap()),
            Event::Create(
                PathBuf::from_str("/test4.txt").unwrap(),
                Item::new_file(0, "test4".to_string(), 10, 1000),
            ),
            Event::Update(
                PathBuf::from_str("/test/test2.txt").unwrap(),
                Item::new_file(1, "test2".to_string(), 10, 1000),
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
            Item::new_dir(0, "test".to_string()),
        );
        let mut snapshot2 = HashMap::new();
        snapshot2.insert(
            PathBuf::from_str("/test").unwrap(),
            Item::new_file(0, "test".to_string(), 100, 1000),
        );

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut expected = vec![
            Event::Delete(PathBuf::from_str("/test").unwrap()),
            Event::Create(
                PathBuf::from_str("/test").unwrap(),
                Item::new_file(0, "test".to_string(), 100, 1000),
            ),
        ];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_update_after_mtime_change() {
        let mut snapshot1 = HashMap::new();
        snapshot1.insert(
            PathBuf::from_str("/test.txt").unwrap(),
            Item::new_file(0, "text".to_string(), 100, 1000),
        );
        let mut snapshot2 = HashMap::new();
        snapshot2.insert(
            PathBuf::from_str("/test.txt").unwrap(),
            Item::new_file(0, "test".to_string(), 101, 1000),
        );

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut expected = vec![Event::Update(
            PathBuf::from_str("/test.txt").unwrap(),
            Item::new_file(1, "test".to_string(), 101, 1000),
        )];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_no_update_after_mtime_change() {
        let hash = "some_text_hash".to_string();

        let mut snapshot1 = HashMap::new();
        let mut file = Item::new_file(0, "text".to_string(), 100, 1000);
        if let ItemKind::File(ref mut metadata) = file.kind {
            metadata.hash = Some(hash.clone());
        }
        snapshot1.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut snapshot2 = HashMap::new();
        let mut file = Item::new_file(0, "text".to_string(), 100, 1000);
        if let ItemKind::File(ref mut metadata) = file.kind {
            metadata.hash = Some(hash.clone());
        }
        snapshot2.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut expected = vec![];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn after_diff_and_apply_state_should_still_have_hash() {
        let hash = "some_text_hash".to_string();

        let mut snapshot1 = HashMap::new();
        let mut file = Item::new_file(0, "test".to_string(), 100, 1000);
        if let ItemKind::File(ref mut metadata) = file.kind {
            metadata.hash = Some(hash.clone());
        }
        snapshot1.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut snapshot2 = HashMap::new();
        let file = Item::new_file(0, "test".to_string(), 101, 1000);
        snapshot2.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut diff = diff_snapshots(&snapshot1, &snapshot2);

        diff.sort();

        let mut item = Item::new_file(1, "test".to_string(), 101, 1000);
        if let ItemKind::File(metadata) = &mut item.kind {
            metadata.hash = Some(hash.clone());
        }

        let mut expected = vec![Event::DirtyUpdate(
            PathBuf::from_str("/test.txt").unwrap(),
            item,
        )];
        expected.sort();

        assert_eq!(diff, expected);

        apply_diff(&mut snapshot1, diff);

        let mut expected = HashMap::new();
        let mut file = Item::new_file(1, "test".to_string(), 101, 1000);
        if let ItemKind::File(ref mut metadata) = file.kind {
            metadata.hash = Some(hash.clone());
        }
        expected.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        assert_eq!(snapshot1, expected);
    }
}
