use crate::sink::{Sink, SinkFileEvent};

#[derive(Clone, Debug)]
pub struct StdoutSink;

impl Sink for StdoutSink {
    async fn handle(&self, event: SinkFileEvent) {
        println!("File event: {event:#?}");
    }
}
