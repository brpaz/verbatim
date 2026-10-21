//! Decodes audio and video files, and captures live audio, into the mono
//! samples the transcriber reads.

use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gst::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not set up decoding: {0}")]
    Setup(String),
    #[error("decoding failed in {element}: {message}")]
    Stream { element: String, message: String },
}

impl From<gst::glib::Error> for Error {
    fn from(err: gst::glib::Error) -> Self {
        Self::Setup(err.to_string())
    }
}

impl From<gst::glib::BoolError> for Error {
    fn from(err: gst::glib::BoolError) -> Self {
        Self::Setup(err.to_string())
    }
}

impl From<gst::StateChangeError> for Error {
    fn from(err: gst::StateChangeError) -> Self {
        Self::Setup(err.to_string())
    }
}

/// GStreamer source of the default input device. PipeWire serves it through
/// its PulseAudio layer, which is also what the Flatpak sandbox exposes.
pub const MICROPHONE: &str = "pulsesrc";

/// GStreamer source of what the default output device plays.
pub const SYSTEM_AUDIO: &str = "pulsesrc device=@DEFAULT_MONITOR@";

/// Initializes GStreamer. Must be called once before [`Decoder::open`].
pub fn init() -> Result<(), Error> {
    Ok(gst::init()?)
}

/// Delivers mono samples at `sample_rate` from a file or a live source, a
/// chunk at a time, so a long file never has to fit in memory.
pub struct Decoder {
    pipeline: PipelineGuard,
    sink: gst_app::AppSink,
    sample_rate: u32,
    /// Cached once known.
    total_samples: Cell<Option<u64>>,
    done: bool,
    stop: Option<Arc<AtomicBool>>,
}

impl Decoder {
    /// Decodes the first audio stream of a file.
    pub fn open(path: &Path, sample_rate: u32) -> Result<Self, Error> {
        let pipeline = launch("filesrc name=src ! decodebin", sample_rate)?;
        pipeline
            .0
            .by_name("src")
            .expect("the launch line names its source")
            .set_property("location", utf8_path(path)?);
        Self::start(pipeline, sample_rate, None)
    }

    /// Captures `source`, a GStreamer source description such as
    /// [`MICROPHONE`]. The stream ends once `stop` is set.
    pub fn capture(source: &str, sample_rate: u32, stop: Arc<AtomicBool>) -> Result<Self, Error> {
        let pipeline = launch(source, sample_rate)?;
        Self::start(pipeline, sample_rate, Some(stop))
    }

    fn start(
        pipeline: PipelineGuard,
        sample_rate: u32,
        stop: Option<Arc<AtomicBool>>,
    ) -> Result<Self, Error> {
        let sink = pipeline
            .0
            .by_name("sink")
            .and_downcast::<gst_app::AppSink>()
            .expect("the launch line names its appsink");

        // A file that cannot be decoded fails here, or on the first chunk.
        // Live sources do not preroll.
        pipeline.0.set_state(gst::State::Paused)?;
        let _ = pipeline.0.state(gst::ClockTime::from_seconds(5));
        pipeline.0.set_state(gst::State::Playing)?;

        let decoder = Self {
            pipeline,
            sink,
            sample_rate,
            total_samples: Cell::new(None),
            done: false,
            stop,
        };
        decoder.total_samples.set(decoder.query_total_samples());
        Ok(decoder)
    }

    /// The duration of a file is usually known once the pipeline has
    /// prerolled, but an MP3 file without a header giving its length only
    /// has one once decoding has started, estimated from its bitrate. Live
    /// sources have no duration.
    fn query_total_samples(&self) -> Option<u64> {
        if self.stop.is_some() {
            return None;
        }
        self.pipeline
            .0
            .query_duration::<gst::ClockTime>()
            .map(|duration| duration.nseconds() * u64::from(self.sample_rate) / 1_000_000_000)
    }

    fn stopped(&self) -> bool {
        self.stop
            .as_ref()
            .is_some_and(|stop| stop.load(Ordering::Relaxed))
    }
}

/// Builds `source ! … ! appsink` converting to mono F32 at `sample_rate`.
fn launch(source: &str, sample_rate: u32) -> Result<PipelineGuard, Error> {
    let pipeline = gst::parse::launch(&format!(
        "{source} ! audioconvert ! audioresample \
         ! audio/x-raw,format=F32LE,channels=1,rate={sample_rate} ! appsink name=sink sync=false"
    ))?
    .downcast::<gst::Pipeline>()
    .expect("a launch line with several elements is a pipeline");
    Ok(PipelineGuard(pipeline))
}

impl Iterator for Decoder {
    type Item = Result<Vec<f32>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let bus = self.pipeline.0.bus().expect("a pipeline always has a bus");
        // A failed pipeline never reaches EOS, so the sink is polled with a
        // timeout and the bus checked for errors in between.
        while !self.done && !self.sink.is_eos() && !self.stopped() {
            if let Some(msg) = bus.pop_filtered(&[gst::MessageType::Error]) {
                self.done = true;
                return Some(Err(stream_error(&msg)));
            }
            let Some(sample) = self
                .sink
                .try_pull_sample(gst::ClockTime::from_mseconds(100))
            else {
                continue;
            };
            let Some(buffer) = sample.buffer() else {
                continue;
            };
            let map = match buffer.map_readable() {
                Ok(map) => map,
                Err(err) => return Some(Err(err.into())),
            };
            let (frames, _) = map.as_slice().as_chunks::<4>();
            return Some(Ok(frames
                .iter()
                .map(|&bytes| f32::from_le_bytes(bytes))
                .collect()));
        }
        None
    }
}

impl verbatim_core::AudioStream for Decoder {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_samples(&self) -> Option<u64> {
        if self.total_samples.get().is_none() {
            self.total_samples.set(self.query_total_samples());
        }
        self.total_samples.get()
    }

    fn next_chunk(&mut self) -> Option<Result<Vec<f32>, verbatim_core::Error>> {
        self.next()
            .map(|chunk| chunk.map_err(|err| verbatim_core::Error::Backend(Box::new(err))))
    }
}

struct PipelineGuard(gst::Pipeline);

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

fn utf8_path(path: &Path) -> Result<&str, Error> {
    path.to_str()
        .ok_or_else(|| Error::Setup(format!("path is not valid UTF-8: {}", path.display())))
}

fn stream_error(msg: &gst::Message) -> Error {
    let element = msg
        .src()
        .map(|src| src.path_string().to_string())
        .unwrap_or_default();
    let message = match msg.view() {
        gst::MessageView::Error(err) => err.error().to_string(),
        _ => format!("unexpected {:?} message", msg.type_()),
    };
    Error::Stream { element, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use verbatim_core::AudioStream;

    /// Writes one second of a 44.1 kHz stereo sine wave to a file, through
    /// `encoder`.
    fn write_sine(path: &Path, encoder: &str) {
        let pipeline = gst::parse::launch(&format!(
            "audiotestsrc num-buffers=43 samplesperbuffer=1024 wave=sine \
             ! audio/x-raw,rate=44100,channels=2 ! audioconvert ! {encoder} \
             ! filesink location={}",
            path.display()
        ))
        .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        let msg = bus
            .timed_pop_filtered(
                gst::ClockTime::from_seconds(10),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            )
            .unwrap();
        assert_eq!(msg.type_(), gst::MessageType::Eos);
        pipeline.set_state(gst::State::Null).unwrap();
    }

    #[test]
    fn decodes_to_mono_chunks_at_requested_rate() {
        init().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sine.ogg");
        write_sine(&path, "vorbisenc ! oggmux");

        let decoder = Decoder::open(&path, 16_000).unwrap();
        let total = decoder.total_samples().unwrap();
        assert_eq!(decoder.sample_rate(), 16_000);
        let chunks: Vec<Vec<f32>> = decoder.map(Result::unwrap).collect();

        assert!(chunks.len() > 1);
        let decoded = chunks.iter().map(Vec::len).sum::<usize>() as i64;
        assert!((decoded - 16_000).abs() < 1_600, "decoded {decoded}");
        assert!(
            (decoded - total as i64).abs() < 1_600,
            "decoded {decoded} of {total}"
        );
        assert!(chunks.iter().flatten().any(|s| s.abs() > 0.1));
    }

    /// `lamemp3enc` writes no header giving the length, as in many podcasts.
    #[test]
    fn finds_duration_of_mp3_without_length_header_once_decoding() {
        init().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sine.mp3");
        write_sine(&path, "lamemp3enc");

        let mut decoder = Decoder::open(&path, 16_000).unwrap();
        let mut decoded = 0;
        for chunk in decoder.by_ref() {
            decoded += chunk.unwrap().len() as i64;
        }
        let total = decoder
            .total_samples()
            .expect("duration known after decoding");

        assert!(
            (decoded - total as i64).abs() < 1_600,
            "decoded {decoded} of {total}"
        );
    }

    #[test]
    fn capture_streams_until_stopped() {
        init().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut capture =
            Decoder::capture("audiotestsrc is-live=true wave=sine", 16_000, stop.clone()).unwrap();

        assert_eq!(capture.total_samples(), None);
        let first = capture.next().unwrap().unwrap();
        assert!(!first.is_empty());
        stop.store(true, Ordering::Relaxed);
        assert!(capture.next().is_none());
    }

    #[test]
    fn rejects_unknown_capture_source() {
        init().unwrap();

        let result = Decoder::capture("nosuchsrc", 16_000, Arc::default());

        assert!(matches!(result, Err(Error::Setup(_))));
    }

    #[test]
    #[ignore = "needs a running PipeWire or PulseAudio server"]
    fn captures_default_devices() {
        init().unwrap();
        for source in [MICROPHONE, SYSTEM_AUDIO] {
            let stop = Arc::new(AtomicBool::new(false));
            let mut capture = Decoder::capture(source, 16_000, stop.clone()).unwrap();

            assert!(capture.next().unwrap().is_ok(), "{source}");
            stop.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn reports_unreadable_file() {
        init().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.ogg");
        std::fs::write(&path, b"not audio").unwrap();

        let result =
            Decoder::open(&path, 16_000).and_then(|decoder| decoder.collect::<Result<Vec<_>, _>>());

        assert!(result.is_err());
    }
}
