use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::{model::Event, sink::stdout_sink::StdoutSink};

pub mod stdout_sink;

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

#[derive(Clone, Debug)]
pub enum SinkKind {
    Stdout(StdoutSink),
}

pub trait Sink {
    async fn handle(&self, event: SinkFileEvent);
}

pub async fn sink_watcher(sink: impl Sink, mut rx: mpsc::Receiver<SinkFileEvent>) {
    while let Some(event) = rx.recv().await {
        sink.handle(event).await
    }
}
