//! Speech recognition backends implementing `verbatim_core::Transcriber`
//! (ADR 0003).

use std::ops::ControlFlow;

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineRecognizerResult,
    OfflineTransducerModelConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};
use verbatim_core::{AudioStream, Error, Event, Segment, Transcriber, Word};
use verbatim_models::{Model, ModelStore, PARAKEET_TDT_V3, SILERO_VAD};

/// Sample rate both the VAD and Parakeet expect.
pub const SAMPLE_RATE: u32 = 16_000;

/// Samples per VAD inference, fixed by the Silero model.
const VAD_WINDOW: usize = 512;

/// Longer speech is split, which keeps transcript lines readable and bounds
/// the memory of a single recognition.
const MAX_SEGMENT_SECS: f32 = 20.0;

/// Silero VAD finds speech, and Parakeet TDT transcribes each speech segment.
pub struct ParakeetTranscriber {
    recognizer: OfflineRecognizer,
    vad_config: VadModelConfig,
}

impl ParakeetTranscriber {
    pub fn load(store: &ModelStore, num_threads: i32) -> Result<Self, Error> {
        for model in [&SILERO_VAD, &PARAKEET_TDT_V3] {
            if !store.is_installed(model) {
                return Err(Error::ModelMissing(model.name.to_owned()));
            }
        }
        let path = |model: &Model, file: &str| {
            Some(store.path(model, file).to_string_lossy().into_owned())
        };

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: path(&PARAKEET_TDT_V3, "encoder.int8.onnx"),
            decoder: path(&PARAKEET_TDT_V3, "decoder.int8.onnx"),
            joiner: path(&PARAKEET_TDT_V3, "joiner.int8.onnx"),
        };
        config.model_config.tokens = path(&PARAKEET_TDT_V3, "tokens.txt");
        config.model_config.num_threads = num_threads;
        config.model_config.provider = Some("cpu".into());
        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| backend_error("could not load the Parakeet model"))?;

        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: path(&SILERO_VAD, "silero_vad.onnx"),
                threshold: 0.5,
                min_silence_duration: 0.25,
                min_speech_duration: 0.25,
                window_size: VAD_WINDOW as i32,
                max_speech_duration: MAX_SEGMENT_SECS,
            },
            ten_vad: Default::default(),
            sample_rate: SAMPLE_RATE as i32,
            num_threads: 1,
            provider: Some("cpu".into()),
            debug: false,
        };

        Ok(Self {
            recognizer,
            vad_config,
        })
    }

    /// Transcribes every speech segment the VAD has completed.
    fn drain(
        &self,
        vad: &VoiceActivityDetector,
        total_samples: Option<u64>,
        on_event: &mut dyn FnMut(Event) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        while let Some(speech) = vad.front() {
            let start = speech.start().max(0) as u64;
            let end = start + speech.samples().len() as u64;

            let stream = self.recognizer.create_stream();
            stream.accept_waveform(SAMPLE_RATE as i32, speech.samples());
            self.recognizer.decode(&stream);
            let result = stream.get_result();
            vad.pop();

            if let Some(segment) = result.and_then(|result| segment(&result, start, end)) {
                on_event(Event::Segment(segment))?;
            }
            if let Some(total) = total_samples.filter(|&total| total > 0) {
                on_event(Event::Progress((end as f32 / total as f32).min(1.0)))?;
            }
        }
        ControlFlow::Continue(())
    }
}

impl Transcriber for ParakeetTranscriber {
    fn transcribe(
        &self,
        audio: &mut dyn AudioStream,
        on_event: &mut dyn FnMut(Event) -> ControlFlow<()>,
    ) -> Result<(), Error> {
        if audio.sample_rate() != SAMPLE_RATE {
            return Err(backend_error(format!(
                "audio must be sampled at {SAMPLE_RATE} Hz, got {}",
                audio.sample_rate()
            )));
        }
        let vad = VoiceActivityDetector::create(&self.vad_config, MAX_SEGMENT_SECS * 2.0)
            .ok_or_else(|| backend_error("could not load the Silero VAD model"))?;

        let total_samples = audio.total_samples();
        // The VAD runs on whole windows; samples left over from one chunk
        // wait for the next.
        let mut pending: Vec<f32> = Vec::new();
        while let Some(chunk) = audio.next_chunk() {
            pending.extend(chunk?);
            let whole = pending.len() / VAD_WINDOW * VAD_WINDOW;
            for window in pending[..whole].chunks(VAD_WINDOW) {
                vad.accept_waveform(window);
            }
            pending.drain(..whole);
            if self.drain(&vad, total_samples, on_event).is_break() {
                return Err(Error::Cancelled);
            }
        }
        if !pending.is_empty() {
            vad.accept_waveform(&pending);
        }
        vad.flush();
        if self.drain(&vad, total_samples, on_event).is_break()
            || on_event(Event::Progress(1.0)).is_break()
        {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

/// Builds a segment spanning samples `start..end` from a recognition result,
/// or `None` when nothing was said.
fn segment(result: &OfflineRecognizerResult, start: u64, end: u64) -> Option<Segment> {
    let text = result.text.trim();
    if text.is_empty() {
        return None;
    }
    let (start_ms, end_ms) = (to_ms(start), to_ms(end));
    Some(Segment {
        start_ms,
        end_ms,
        text: text.to_owned(),
        words: words(result, start_ms, end_ms),
    })
}

/// Groups subword tokens into words. A token starting with a space (or the
/// SentencePiece marker "▁") begins a new word. Token timestamps are seconds
/// from the start of the segment.
fn words(result: &OfflineRecognizerResult, start_ms: u64, end_ms: u64) -> Vec<Word> {
    let Some(timestamps) = &result.timestamps else {
        return Vec::new();
    };
    if timestamps.len() != result.tokens.len() {
        return Vec::new();
    }
    let at = |secs: f32| (start_ms + (secs.max(0.0) * 1000.0) as u64).min(end_ms);

    let mut words: Vec<Word> = Vec::new();
    for (i, token) in result.tokens.iter().enumerate() {
        let starts_word = token.starts_with([' ', '▁']);
        let piece = token.trim_start_matches([' ', '▁']);
        let token_end = match &result.durations {
            Some(durations) => at(timestamps[i] + durations.get(i).copied().unwrap_or(0.0)),
            None => timestamps.get(i + 1).map_or(end_ms, |&next| at(next)),
        };
        match words.last_mut() {
            Some(word) if !starts_word => {
                word.text.push_str(piece);
                word.end_ms = token_end;
            }
            _ if piece.is_empty() => {}
            _ => words.push(Word {
                start_ms: at(timestamps[i]),
                end_ms: token_end,
                text: piece.to_owned(),
            }),
        }
    }
    words
}

fn to_ms(samples: u64) -> u64 {
    samples * 1000 / u64::from(SAMPLE_RATE)
}

fn backend_error(message: impl Into<String>) -> Error {
    Error::Backend(message.into().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(
        tokens: &[&str],
        timestamps: &[f32],
        durations: Option<&[f32]>,
    ) -> OfflineRecognizerResult {
        OfflineRecognizerResult {
            text: tokens.concat(),
            tokens: tokens.iter().map(|t| t.to_string()).collect(),
            timestamps: Some(timestamps.to_vec()),
            durations: durations.map(<[f32]>::to_vec),
        }
    }

    #[test]
    fn groups_tokens_into_timed_words() {
        let result = result(
            &[" Hel", "lo", ",", " wor", "ld", "."],
            &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0],
            Some(&[0.2, 0.2, 0.1, 0.2, 0.2, 0.1]),
        );

        let words = words(&result, 5_000, 7_000);

        assert_eq!(
            words,
            [
                Word {
                    start_ms: 5_000,
                    end_ms: 5_500,
                    text: "Hello,".into()
                },
                Word {
                    start_ms: 5_600,
                    end_ms: 6_100,
                    text: "world.".into()
                },
            ]
        );
    }

    #[test]
    fn word_ends_at_next_token_without_durations() {
        let result = result(&["▁yes", "▁no"], &[0.0, 0.5], None);

        let words = words(&result, 0, 1_200);

        assert_eq!(
            words,
            [
                Word {
                    start_ms: 0,
                    end_ms: 500,
                    text: "yes".into()
                },
                Word {
                    start_ms: 500,
                    end_ms: 1_200,
                    text: "no".into()
                },
            ]
        );
    }

    #[test]
    fn mismatched_timestamps_give_no_words() {
        let result = result(&[" a", " b"], &[0.0], None);

        assert!(words(&result, 0, 1_000).is_empty());
    }

    /// Runs against real models: set `VERBATIM_MODELS_DIR` to an installed
    /// model store, and `VERBATIM_TEST_AUDIO` to a file with English speech.
    #[test]
    #[ignore = "needs installed models in VERBATIM_MODELS_DIR"]
    fn transcribes_speech_with_word_timings() {
        let dir = std::env::var("VERBATIM_MODELS_DIR").expect("VERBATIM_MODELS_DIR is set");
        let audio = std::env::var("VERBATIM_TEST_AUDIO").expect("VERBATIM_TEST_AUDIO is set");
        verbatim_audio::init().unwrap();
        let mut decoder =
            verbatim_audio::Decoder::open(std::path::Path::new(&audio), SAMPLE_RATE).unwrap();
        let transcriber = ParakeetTranscriber::load(&ModelStore::new(&dir), 2).unwrap();
        let mut segments = Vec::new();
        let mut last_progress = 0.0;

        transcriber
            .transcribe(&mut decoder, &mut |event| {
                match event {
                    Event::Segment(segment) => segments.push(segment),
                    Event::Progress(p) => last_progress = p,
                }
                ControlFlow::Continue(())
            })
            .unwrap();

        assert!(!segments.is_empty());
        assert!(segments.windows(2).all(|w| w[0].end_ms <= w[1].start_ms));
        for segment in &segments {
            assert!(!segment.words.is_empty());
            let joined: Vec<_> = segment.words.iter().map(|w| w.text.as_str()).collect();
            assert_eq!(joined.join(" "), segment.text);
            assert!(segment.words.iter().all(|w| {
                segment.start_ms <= w.start_ms
                    && w.start_ms <= w.end_ms
                    && w.end_ms <= segment.end_ms
            }));
        }
        assert_eq!(last_progress, 1.0);
        eprintln!("{segments:#?}");
    }

    #[test]
    #[ignore = "needs installed models in VERBATIM_MODELS_DIR"]
    fn stops_when_cancelled() {
        let dir = std::env::var("VERBATIM_MODELS_DIR").expect("VERBATIM_MODELS_DIR is set");
        let audio = std::env::var("VERBATIM_TEST_AUDIO").expect("VERBATIM_TEST_AUDIO is set");
        verbatim_audio::init().unwrap();
        let mut decoder =
            verbatim_audio::Decoder::open(std::path::Path::new(&audio), SAMPLE_RATE).unwrap();
        let transcriber = ParakeetTranscriber::load(&ModelStore::new(&dir), 2).unwrap();

        let result = transcriber.transcribe(&mut decoder, &mut |_| ControlFlow::Break(()));

        assert!(matches!(result, Err(Error::Cancelled)));
    }

    #[test]
    fn rejects_missing_models() {
        let root = std::env::temp_dir().join("verbatim-no-models");

        let result = ParakeetTranscriber::load(&ModelStore::new(root), 1);

        assert!(matches!(result, Err(Error::ModelMissing(_))));
    }
}
