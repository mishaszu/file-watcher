use tokio::sync::mpsc;

use crate::{model::FileEvent, sink::stdout_sink::StdoutSink};

pub mod stdout_sink;

#[derive(Clone, Debug)]
pub enum SinkKind {
    Stdout(StdoutSink),
}

trait Sink {
    async fn handle(&self, event: FileEvent);
}

pub async fn sink_watcher(sink: impl Sink, mut rx: mpsc::Receiver<FileEvent>) {
    while let Some(event) = rx.recv().await.take() {
        sink.handle(event).await
    }
}
