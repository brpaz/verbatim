//! Runs a transcription on a background thread inside the app process
//! (ADR 0005), reporting to the main loop through a channel.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use gtk::glib;
use verbatim_core::{Error, Event, Segment, Transcriber};
use verbatim_models::{Model, ModelStore, TRANSCRIBE_MODELS};
use verbatim_transcribe::{ParakeetTranscriber, SAMPLE_RATE};

#[derive(Debug)]
pub enum Update {
    /// Fraction of the model download done.
    Downloading(f32),
    Transcribing(f32),
    Segment(Segment),
    Finished,
    Cancelled,
    Failed(String),
}

pub fn model_store() -> ModelStore {
    ModelStore::new(glib::user_data_dir().join("verbatim").join("models"))
}

pub fn missing_models(store: &ModelStore) -> Vec<&'static Model> {
    store.missing(&TRANSCRIBE_MODELS)
}

/// A running transcription. Dropping it does not stop the thread; call
/// [`Job::cancel`].
#[derive(Debug)]
pub struct Job {
    cancelled: Arc<AtomicBool>,
}

impl Job {
    /// Transcribes `path`, downloading missing models first. Updates end
    /// with exactly one of `Finished`, `Cancelled` or `Failed`.
    pub fn start(path: PathBuf, sender: async_channel::Sender<Update>) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        thread::spawn(move || {
            let last = match run(&path, &flag, &sender) {
                Ok(()) => Update::Finished,
                Err(Failure::Cancelled) => Update::Cancelled,
                Err(Failure::Other(message)) => {
                    tracing::warn!(path = %path.display(), error = %message, "transcription failed");
                    Update::Failed(message)
                }
            };
            let _ = sender.send_blocking(last);
        });
        Self { cancelled }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

enum Failure {
    Cancelled,
    Other(String),
}

impl From<Error> for Failure {
    fn from(err: Error) -> Self {
        match err {
            Error::Cancelled => Failure::Cancelled,
            err => Failure::Other(err.to_string()),
        }
    }
}

fn run(
    path: &Path,
    cancelled: &AtomicBool,
    sender: &async_channel::Sender<Update>,
) -> Result<(), Failure> {
    // A closed channel means the window is gone, which also cancels.
    let send = |update| {
        if cancelled.load(Ordering::Relaxed) || sender.send_blocking(update).is_err() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };

    let store = model_store();
    store
        .install(&TRANSCRIBE_MODELS, |done, total| {
            send(Update::Downloading(done as f32 / total.max(1) as f32))
        })
        .map_err(|err| match err {
            verbatim_models::Error::Cancelled => Failure::Cancelled,
            err => Failure::Other(err.to_string()),
        })?;

    let _ = send(Update::Transcribing(0.0));
    let threads = thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
    let transcriber = ParakeetTranscriber::load(&store, threads as i32)?;
    verbatim_audio::init().map_err(|err| Failure::Other(err.to_string()))?;
    let mut audio = verbatim_audio::Decoder::open(path, SAMPLE_RATE)
        .map_err(|err| Failure::Other(err.to_string()))?;

    transcriber.transcribe(&mut audio, &mut |event| match event {
        Event::Progress(fraction) => send(Update::Transcribing(fraction)),
        Event::Segment(segment) => send(Update::Segment(segment)),
    })?;
    Ok(())
}
