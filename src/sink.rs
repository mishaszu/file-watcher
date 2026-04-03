use tokio::sync::mpsc;

use crate::{model::SinkFileEvent, sink::stdout_sink::StdoutSink};

pub mod stdout_sink;

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
