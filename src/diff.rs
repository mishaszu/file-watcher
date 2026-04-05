use std::path::PathBuf;

use crate::{
    Snapshot,
    model::{Event, EventError, Hash, Item, ItemKind},
};

pub fn diff_snapshots(old: &Snapshot, new: &Snapshot, next_job_id: &mut u64) -> Vec<Event> {
    let mut events = Vec::new();

    for (path, new_item) in new.iter() {
        if let Some(old_item) = old.get(path) {
            let new_version = old_item.version + 1;
            let comparison_events: Vec<Event> =
                compare_items(path, old_item, new_item, new_version, next_job_id)
                    .into_iter()
                    .flatten()
                    .collect();
            events.extend_from_slice(&comparison_events);
        } else {
            match new_item.kind {
                ItemKind::Dir(_) => {
                    events.push(Event::Create(path.to_owned(), new_item.clone()));
                }
                ItemKind::File(_) => {
                    let mut item = new_item.clone();
                    item.update_hash(Hash::PendingNew(next_job_id.to_owned()));
                    *next_job_id += 1;
                    events.push(Event::Create(path.to_owned(), item));
                }
            }
        }
    }

    for (path, _) in old.iter() {
        if !new.contains_key(path) {
            events.push(Event::Delete(path.to_owned()));
        }
    }

    events
}

pub fn compare_items(
    path: &PathBuf,
    old_item: &Item,
    new_item: &Item,
    new_version: u64,
    next_job_id: &mut u64,
) -> Option<Vec<Event>> {
    match (&new_item.kind, &old_item.kind) {
        (ItemKind::File(new_file_metadata), ItemKind::File(old_file_metadata)) => {
            if new_file_metadata.size != old_file_metadata.size {
                // size changed, need to calculate new hash but doesn't need hash
                // comparison
                let item = Item::new_file_with_update_hash(
                    new_version,
                    new_file_metadata.name.clone(),
                    new_file_metadata.mtime,
                    new_file_metadata.size,
                    next_job_id.to_owned(),
                );
                *next_job_id += 1;

                Some(Vec::from([Event::Update(path.to_owned(), item)]))
            } else if new_file_metadata.mtime != old_file_metadata.mtime {
                let mut item = new_item.to_owned();
                item.version = new_version;

                let job_id = next_job_id.to_owned();
                *next_job_id += 1;

                match &old_file_metadata.hash {
                    // Naive approach. Always update if no hash and mtime change.
                    // With async hash calculating would need another
                    // approach to sync hashes before diff to make proper comparison
                    Hash::PendingNew(_) | Hash::None => {
                        item.update_hash(Hash::PendingNew(job_id));
                        Some(Vec::from([Event::Update(path.to_owned(), item)]))
                    }
                    // if old hash was present new hash have to be generated to compare
                    // for change
                    Hash::Pending(old_hash, _) | Hash::Computed(old_hash) => {
                        item.update_hash(Hash::Pending(old_hash.clone(), job_id));
                        Some(Vec::from([Event::DirtyUpdate(path.to_owned(), item)]))
                    }
                }
            } else {
                None
            }
        }
        (ItemKind::File(metadata), ItemKind::Dir(_)) => {
            let ev1 = Event::Delete(path.to_owned());
            let item = Item::new_file_with_update_hash(
                new_version,
                metadata.name.clone(),
                metadata.mtime,
                metadata.size,
                next_job_id.to_owned(),
            );
            *next_job_id += 1;
            let ev2 = Event::Create(path.to_owned(), item);
            Some(Vec::from([ev1, ev2]))
        }
        (ItemKind::Dir(metadata), ItemKind::File(_)) => {
            let ev1 = Event::Delete(path.to_owned());
            let ev2 = Event::Create(
                path.to_owned(),
                Item::new_dir(new_version, metadata.name.clone()),
            );

            Some(Vec::from([ev1, ev2]))
        }
        _ => None,
    }
}

pub fn find_n_diff_item(
    old: &Snapshot,
    new_item: (PathBuf, ItemKind),
    next_job_id: &mut u64,
) -> Vec<Event> {
    if let Some(old_item) = old.get(&new_item.0) {
        let new_version = old_item.version + 1;
        compare_items(
            &new_item.0,
            old_item,
            &Item {
                version: 0,
                kind: new_item.1,
            },
            new_version,
            next_job_id,
        )
        .into_iter()
        .flatten()
        .collect()
    } else {
        let mut kind = new_item.1;
        match &mut kind {
            ItemKind::File(file_metadata) => {
                file_metadata.hash = Hash::PendingNew(next_job_id.to_owned());
            }
            ItemKind::Dir(_) => (),
        };
        *next_job_id += 1;
        Vec::from([Event::Create(new_item.0, Item { version: 0, kind })])
    }
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
        model::{Event, Hash, Item, ItemKind},
    };

    fn find_and_compare(events: &[Event], expect: &Event) {
        let event = events
            .iter()
            .find(|&event| event.compare_path(expect.get_path()))
            .unwrap();
        match (event, expect) {
            (Event::Create(_, item), Event::Create(_, expected))
            | (Event::Update(_, item), Event::Update(_, expected))
            | (Event::DirtyUpdate(_, item), Event::DirtyUpdate(_, expected)) => {
                match (&item.kind, &expected.kind) {
                    (ItemKind::File(item_medatada), ItemKind::File(expect_metadata)) => {
                        assert_eq!(
                            (
                                &item_medatada.name,
                                &item_medatada.mtime,
                                &item_medatada.size
                            ),
                            (
                                &expect_metadata.name,
                                &expect_metadata.mtime,
                                &expect_metadata.size
                            ),
                        );
                        match (&item_medatada.hash, &expect_metadata.hash) {
                            (Hash::None, Hash::None)
                            | (Hash::PendingNew(_), Hash::PendingNew(_)) => (),
                            (Hash::Pending(item_hash, _), Hash::Pending(expected_hash, _))
                                if item_hash == expected_hash => {}
                            (Hash::Computed(item_hash), Hash::Computed(expected_hash))
                                if item_hash == expected_hash => {}
                            _ => panic!(
                                "Expected item doesn't match snapshot item hash. {:#?}: {:#?}",
                                item_medatada, expect_metadata
                            ),
                        }
                    }
                    (ItemKind::Dir(item_medatada), ItemKind::Dir(expect_metadata)) => {
                        assert_eq!(&item_medatada.name, &expect_metadata.name,);
                    }
                    _ => panic!(
                        "Expected item doesn't match snapshot item kind. {:#?}: {:#?}",
                        item, expected
                    ),
                }
            }
            (Event::Delete(_), Event::Delete(_)) => (),
            _ => panic!("Expected event doesn't much snapshot event"),
        }
    }

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

        let mut next_job = 0;

        let diff = diff_snapshots(&snapshot1, &snapshot2, &mut next_job);

        find_and_compare(
            &diff,
            &Event::Create(
                PathBuf::from_str("/test5.txt").unwrap(),
                Item::new_file_with_update_hash(0, "test5".to_string(), 10, 1000, 0),
            ),
        );
        find_and_compare(
            &diff,
            &Event::Delete(PathBuf::from_str("/test4.txt").unwrap()),
        );
        find_and_compare(
            &diff,
            &Event::Update(
                PathBuf::from_str("/test/test2.txt").unwrap(),
                Item::new_file_with_update_hash(1, "test2".to_string(), 11, 1030, 0),
            ),
        );

        assert_eq!(next_job, 2);
    }

    #[test]
    fn diff_snapshots_2() {
        let snapshot1 = create_snapshot_2();
        let snapshot2 = create_snapshot_1();

        let mut next_job = 0;

        let mut diff = diff_snapshots(&snapshot1, &snapshot2, &mut next_job);

        diff.sort();

        find_and_compare(
            &diff,
            &Event::Delete(PathBuf::from_str("/test5.txt").unwrap()),
        );
        find_and_compare(
            &diff,
            &Event::Create(
                PathBuf::from_str("/test4.txt").unwrap(),
                Item::new_file_with_update_hash(0, "test4".to_string(), 10, 1000, 0),
            ),
        );
        find_and_compare(
            &diff,
            &Event::Update(
                PathBuf::from_str("/test/test2.txt").unwrap(),
                Item::new_file_with_update_hash(1, "test2".to_string(), 10, 1000, 0),
            ),
        );

        assert_eq!(next_job, 2);
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

        let mut next_job = 0;
        let mut diff = diff_snapshots(&snapshot1, &snapshot2, &mut next_job);

        let (first, last) = diff.split_at_mut(1);

        find_and_compare(first, &Event::Delete(PathBuf::from_str("/test").unwrap()));
        find_and_compare(
            last,
            &Event::Create(
                PathBuf::from_str("/test").unwrap(),
                Item::new_file_with_update_hash(0, "test".to_string(), 100, 1000, 0),
            ),
        );

        assert_eq!(next_job, 1);
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

        let mut next_job = 0;
        let mut diff = diff_snapshots(&snapshot1, &snapshot2, &mut next_job);

        diff.sort();

        let mut expected = vec![Event::Update(
            PathBuf::from_str("/test.txt").unwrap(),
            Item::new_file_with_update_hash(1, "test".to_string(), 101, 1000, 0),
        )];
        expected.sort();

        assert_eq!(diff, expected);
    }

    #[test]
    fn diff_no_update_after_mtime_change() {
        let hash = "some_text_hash".to_string();

        let mut snapshot1 = HashMap::new();
        let mut file = Item::new_file(0, "text".to_string(), 100, 1000);
        file.update_hash(Hash::Computed(hash.clone()));
        snapshot1.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut snapshot2 = HashMap::new();
        let mut file = Item::new_file(0, "text".to_string(), 100, 1000);
        file.update_hash(Hash::Computed(hash.clone()));
        snapshot2.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut next_job = 0;

        let mut diff = diff_snapshots(&snapshot1, &snapshot2, &mut next_job);

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
        file.update_hash(Hash::Computed(hash.clone()));
        snapshot1.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut snapshot2 = HashMap::new();
        let file = Item::new_file(0, "test".to_string(), 101, 1000);
        snapshot2.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        let mut job_id = 0;
        let mut diff = diff_snapshots(&snapshot1, &snapshot2, &mut job_id);

        diff.sort();

        let mut item = Item::new_file(1, "test".to_string(), 101, 1000);
        item.update_hash(Hash::Pending(hash.clone(), 0));

        let mut expected = vec![Event::DirtyUpdate(
            PathBuf::from_str("/test.txt").unwrap(),
            item,
        )];
        expected.sort();

        assert_eq!(diff, expected);

        apply_diff(&mut snapshot1, diff);

        let mut expected = HashMap::new();
        let mut file = Item::new_file(1, "test".to_string(), 101, 1000);
        file.update_hash(Hash::Pending(hash.clone(), 0));
        expected.insert(PathBuf::from_str("/test.txt").unwrap(), file);

        assert_eq!(snapshot1, expected);
        assert_eq!(job_id, 1);
    }
}
