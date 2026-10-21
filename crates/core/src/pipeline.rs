use std::ops::ControlFlow;

use crate::transcript::Segment;

/// Decoded mono PCM samples in `[-1.0, 1.0]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Audio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("model not installed: {0}")]
    ModelMissing(String),
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync>),
}

/// Mono samples delivered a chunk at a time, so long files need not fit in
/// memory.
pub trait AudioStream {
    fn sample_rate(&self) -> u32;
    /// Length of the whole stream in samples, when known. It can become
    /// known only after the first chunks have been read.
    fn total_samples(&self) -> Option<u64>;
    /// The next chunk, or `None` at the end.
    fn next_chunk(&mut self) -> Option<Result<Vec<f32>, Error>>;
}

impl Audio {
    /// Streams these samples in chunks of `chunk` samples.
    pub fn stream(&self, chunk: usize) -> impl AudioStream + '_ {
        AudioChunks {
            audio: self,
            chunks: self.samples.chunks(chunk.max(1)),
        }
    }
}

struct AudioChunks<'a> {
    audio: &'a Audio,
    chunks: std::slice::Chunks<'a, f32>,
}

impl AudioStream for AudioChunks<'_> {
    fn sample_rate(&self) -> u32 {
        self.audio.sample_rate
    }

    fn total_samples(&self) -> Option<u64> {
        Some(self.audio.samples.len() as u64)
    }

    fn next_chunk(&mut self) -> Option<Result<Vec<f32>, Error>> {
        self.chunks.next().map(|chunk| Ok(chunk.to_vec()))
    }
}

/// What a transcriber reports while it runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Fraction of the stream processed, in `[0.0, 1.0]`, when its length is
    /// known.
    Progress(f32),
    /// Text recognised so far in the segment still being spoken, replaced by
    /// the next `Partial` or by the finished `Segment`. Only transcribers
    /// asked for live results send it.
    Partial(String),
    /// A finished segment. Segments arrive in time order.
    Segment(Segment),
}

pub trait Transcriber {
    /// Transcribes `audio`, reporting each segment as soon as it is
    /// recognised. Returning `ControlFlow::Break` from `on_event` stops the
    /// transcription with [`Error::Cancelled`].
    fn transcribe(
        &self,
        audio: &mut dyn AudioStream,
        on_event: &mut dyn FnMut(Event) -> ControlFlow<()>,
    ) -> Result<(), Error>;
}
