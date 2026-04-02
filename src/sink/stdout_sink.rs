use crate::{model::FileEvent, sink::Sink};

#[derive(Clone, Debug)]
pub struct StdoutSink;

impl Sink for StdoutSink {
    async fn handle(&self, event: FileEvent) {
        println!("File event: {event:#?}");
    }
}
