//! Runs a transcription of a file or of live audio on a background thread
//! inside the app process (ADR 0005), reporting to the main loop through a
//! channel.

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use gtk::{gio, glib, prelude::*};
use verbatim_core::{Error, Event, Segment, Transcriber};
use verbatim_models::{Model, ModelStore};
pub use verbatim_transcribe::Engine;
use verbatim_transcribe::{SAMPLE_RATE, SherpaTranscriber};

#[derive(Debug, Clone)]
pub enum Input {
    File(PathBuf),
    Live(LiveSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveSource {
    Microphone,
    SystemAudio,
}

impl LiveSource {
    /// Name stored in the `live-source` setting and used as action target.
    pub fn id(self) -> &'static str {
        match self {
            LiveSource::Microphone => "microphone",
            LiveSource::SystemAudio => "system",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "microphone" => Some(LiveSource::Microphone),
            "system" => Some(LiveSource::SystemAudio),
            _ => None,
        }
    }

    fn gst_source(self) -> &'static str {
        match self {
            LiveSource::Microphone => verbatim_audio::MICROPHONE,
            LiveSource::SystemAudio => verbatim_audio::SYSTEM_AUDIO,
        }
    }
}

#[derive(Debug)]
pub enum Update {
    /// Fraction of the model download done.
    Downloading(f32),
    Transcribing(f32),
    /// Live capture has started.
    Listening,
    /// Speech in progress, replaced by the next `Partial` or `Segment`.
    Partial(String),
    Segment(Segment),
    Finished,
    Cancelled,
    Failed(String),
}

pub fn model_store() -> ModelStore {
    ModelStore::new(glib::user_data_dir().join("verbatim").join("models"))
}

pub fn missing_models(store: &ModelStore, engine: &Engine) -> Vec<&'static Model> {
    store.missing(engine.models())
}

/// The engine chosen in the `speech-model` and `language` settings.
pub fn engine_from_settings(settings: &gio::Settings) -> Engine {
    match settings.string("speech-model").as_str() {
        "whisper" => {
            let language = settings.string("language");
            Engine::Whisper {
                language: (!language.is_empty()).then(|| language.to_string()),
            }
        }
        _ => Engine::Parakeet,
    }
}

/// A running transcription. Dropping it does not stop the thread; call
/// [`Job::cancel`] or [`Job::stop`].
#[derive(Debug)]
pub struct Job {
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

impl Job {
    /// Transcribes `input`, downloading missing models first. Updates end
    /// with exactly one of `Finished`, `Cancelled` or `Failed`.
    pub fn start(input: Input, engine: Engine, sender: async_channel::Sender<Update>) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let flags = (cancelled.clone(), stopped.clone());
        thread::spawn(move || {
            let last = match run(&input, &engine, &flags.0, flags.1, &sender) {
                Ok(()) => Update::Finished,
                Err(Failure::Cancelled) => Update::Cancelled,
                Err(Failure::Other(message)) => {
                    tracing::warn!(?input, error = %message, "transcription failed");
                    Update::Failed(message)
                }
            };
            let _ = sender.send_blocking(last);
        });
        Self { cancelled, stopped }
    }

    /// Stops at once, dropping speech not yet transcribed.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Ends live capture. Speech already captured is still transcribed
    /// before the job finishes.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
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
    input: &Input,
    engine: &Engine,
    cancelled: &AtomicBool,
    stopped: Arc<AtomicBool>,
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
        .install(engine.models(), |done, total| {
            send(Update::Downloading(done as f32 / total.max(1) as f32))
        })
        .map_err(|err| match err {
            verbatim_models::Error::Cancelled => Failure::Cancelled,
            err => Failure::Other(err.to_string()),
        })?;

    let _ = send(Update::Transcribing(0.0));
    let threads = thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
    let mut transcriber = SherpaTranscriber::load(&store, engine, threads as i32)?;
    verbatim_audio::init().map_err(audio_failure)?;
    let mut audio = match input {
        Input::File(path) => {
            verbatim_audio::Decoder::open(path, SAMPLE_RATE).map_err(audio_failure)?
        }
        Input::Live(source) => {
            transcriber = transcriber.with_partials(engine.partial_interval());
            let capture =
                verbatim_audio::Decoder::capture(source.gst_source(), SAMPLE_RATE, stopped)
                    .map_err(audio_failure)?;
            let _ = send(Update::Listening);
            capture
        }
    };

    transcriber.transcribe(&mut audio, &mut |event| match event {
        Event::Progress(fraction) => send(Update::Transcribing(fraction)),
        Event::Partial(text) => send(Update::Partial(text)),
        Event::Segment(segment) => send(Update::Segment(segment)),
    })?;
    Ok(())
}

fn audio_failure(err: verbatim_audio::Error) -> Failure {
    Failure::Other(err.to_string())
}
