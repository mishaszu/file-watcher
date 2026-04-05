use std::path::PathBuf;

use notify::Event;

use crate::model::ItemKind;

#[derive(Debug, Clone)]
pub enum OperationNeeded {
    Scan(PathBuf),
    Delete(PathBuf),
}

pub fn accept_event(event: &Event) -> Option<OperationNeeded> {
    if event.paths.iter().any(|path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with('~'))
            .unwrap_or(false)
    }) || event.paths.is_empty()
    {
        return None;
    }

    let path = event.paths.first().unwrap();
    match event.kind {
        notify::EventKind::Create(notify::event::CreateKind::File)
        | notify::EventKind::Create(notify::event::CreateKind::Folder) => {
            Some(OperationNeeded::Scan(path.to_owned()))
        }
        notify::EventKind::Modify(modify_kind) => match modify_kind {
            notify::event::ModifyKind::Data(notify::event::DataChange::Size)
            | notify::event::ModifyKind::Metadata(notify::event::MetadataKind::WriteTime) => {
                Some(OperationNeeded::Scan(path.to_owned()))
            }
            notify::event::ModifyKind::Name(_) => {
                // TODO: rename not in scope for now
                None
            }
            _ => None,
        },
        notify::EventKind::Remove(remove_kind) => match remove_kind {
            notify::event::RemoveKind::Any | notify::event::RemoveKind::Other => None,
            notify::event::RemoveKind::File | notify::event::RemoveKind::Folder => {
                Some(OperationNeeded::Delete(path.to_owned()))
            }
        },
        _ => None,
    }
}

pub enum WatcherMsg {
    ItemChange((PathBuf, ItemKind)),
    Delete(PathBuf),
}
