use std::pin::Pin;

use futures::Stream;

use crate::model::StreamChunk;

pub type ModelStream = Pin<Box<dyn Stream<Item = StreamChunk> + Send>>;
