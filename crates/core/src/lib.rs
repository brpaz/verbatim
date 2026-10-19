//! Domain types, the transcription contract and transcript exports. Depends
//! on no inference or audio library.

mod export;
mod pipeline;
mod transcript;

pub use export::{Format, render};
pub use pipeline::{Audio, AudioStream, Error, Event, Transcriber};
pub use transcript::{Segment, Word, clock_time};
