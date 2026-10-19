//! Speech recognition with sherpa-onnx, implementing
//! `verbatim_core::Transcriber` (ADR 0003, ADR 0008).

use std::ops::ControlFlow;
use std::time::Duration;

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineRecognizerResult,
    OfflineTransducerModelConfig, OfflineWhisperModelConfig, SileroVadModelConfig, VadModelConfig,
    VoiceActivityDetector,
};
use verbatim_core::{AudioStream, Error, Event, Segment, Transcriber, Word};
use verbatim_models::{Model, ModelStore, PARAKEET_TDT_V3, SILERO_VAD, WHISPER_LARGE_V3_TURBO};

/// Sample rate the VAD and both recognisers expect.
pub const SAMPLE_RATE: u32 = 16_000;

/// Samples per VAD inference, fixed by the Silero model.
const VAD_WINDOW: usize = 512;

/// Longer speech is split, which keeps transcript lines readable and bounds
/// the memory of a single recognition.
const MAX_SEGMENT_SECS: f32 = 20.0;

/// Live captions split sooner, so a long run of speech becomes final text
/// sooner and partial recognitions stay cheap.
const LIVE_MAX_SEGMENT_SECS: f32 = 10.0;

/// Live captions wait for a longer pause before ending a line, so short
/// replies are less often cut off from the speech around them.
const LIVE_MIN_SILENCE_SECS: f32 = 0.5;

/// Parakeet detects the language from the audio it is given, and often
/// guesses wrong on a word or two said alone. Speech shorter than this is
/// recognised after the end of the previous segment, whose words are then
/// dropped.
const SHORT_SPEECH_SECS: f32 = 6.0;
const CONTEXT_SECS: f32 = 3.0;
/// Silence between the context and the speech, which keeps their words
/// apart.
const CONTEXT_GAP_SECS: f32 = 0.2;

static PARAKEET_MODELS: [&Model; 2] = [&SILERO_VAD, &PARAKEET_TDT_V3];
static WHISPER_MODELS: [&Model; 2] = [&SILERO_VAD, &WHISPER_LARGE_V3_TURBO];

/// The speech recognition model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// NVIDIA Parakeet TDT v3: fast, 25 European languages, detected from
    /// the audio, with word timings.
    Parakeet,
    /// OpenAI Whisper large-v3-turbo: about 100 languages, slower. Detects
    /// the language unless one is given as an ISO 639-1 code such as "pt".
    /// This export gives no word timings.
    Whisper { language: Option<String> },
}

impl Engine {
    /// Models this engine needs, the VAD included.
    pub fn models(&self) -> &'static [&'static Model] {
        match self {
            Engine::Parakeet => &PARAKEET_MODELS,
            Engine::Whisper { .. } => &WHISPER_MODELS,
        }
    }

    /// How often live captions can re-decode the speech in progress and
    /// keep up with real time on a laptop CPU. Whisper always encodes a
    /// 30-second window, so each decode costs about as much as a long one.
    pub fn partial_interval(&self) -> Duration {
        match self {
            Engine::Parakeet => Duration::from_secs(1),
            Engine::Whisper { .. } => Duration::from_secs(2),
        }
    }
}

/// Silero VAD finds speech, and the engine's model transcribes each speech
/// segment.
pub struct SherpaTranscriber {
    recognizer: OfflineRecognizer,
    vad_config: VadModelConfig,
    /// Samples between partial recognitions, when live results are wanted.
    partial_every: Option<usize>,
    /// Whether short speech is recognised after context (see
    /// [`SHORT_SPEECH_SECS`]), which needs word timings to drop the
    /// context's words, and only helps when the model detects the language.
    uses_context: bool,
}

impl SherpaTranscriber {
    pub fn load(store: &ModelStore, engine: &Engine, num_threads: i32) -> Result<Self, Error> {
        for model in engine.models() {
            if !store.is_installed(model) {
                return Err(Error::ModelMissing(model.name.to_owned()));
            }
        }
        let path = |model: &Model, file: &str| {
            Some(store.path(model, file).to_string_lossy().into_owned())
        };

        let mut config = OfflineRecognizerConfig::default();
        match engine {
            Engine::Parakeet => {
                config.model_config.transducer = OfflineTransducerModelConfig {
                    encoder: path(&PARAKEET_TDT_V3, "encoder.int8.onnx"),
                    decoder: path(&PARAKEET_TDT_V3, "decoder.int8.onnx"),
                    joiner: path(&PARAKEET_TDT_V3, "joiner.int8.onnx"),
                };
                config.model_config.tokens = path(&PARAKEET_TDT_V3, "tokens.txt");
            }
            Engine::Whisper { language } => {
                let model = &WHISPER_LARGE_V3_TURBO;
                config.model_config.whisper = OfflineWhisperModelConfig {
                    encoder: path(model, "turbo-encoder.int8.onnx"),
                    decoder: path(model, "turbo-decoder.int8.onnx"),
                    // An empty language makes Whisper detect it.
                    language: Some(language.clone().unwrap_or_default()),
                    task: Some("transcribe".into()),
                    tail_paddings: -1,
                    enable_token_timestamps: false,
                    enable_segment_timestamps: false,
                };
                config.model_config.tokens = path(model, "turbo-tokens.txt");
            }
        }
        config.model_config.num_threads = num_threads;
        config.model_config.provider = Some("cpu".into());
        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| backend_error("could not load the speech recognition model"))?;

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
            partial_every: None,
            uses_context: *engine == Engine::Parakeet,
        })
    }

    /// Sends [`Event::Partial`] with the speech in progress every `interval`
    /// of audio, for live captions (ADR 0007).
    pub fn with_partials(mut self, interval: Duration) -> Self {
        self.partial_every = Some((interval.as_secs_f32() * SAMPLE_RATE as f32) as usize);
        self.vad_config.silero_vad.max_speech_duration = LIVE_MAX_SEGMENT_SECS;
        self.vad_config.silero_vad.min_silence_duration = LIVE_MIN_SILENCE_SECS;
        self
    }

    fn recognize(&self, samples: &[f32]) -> Option<OfflineRecognizerResult> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.recognizer.decode(&stream);
        stream.get_result()
    }

    /// Recognises speech spanning samples `start..end` of the stream, after
    /// `context` when the speech is short.
    fn recognize_in_context(
        &self,
        speech: &[f32],
        context: &[f32],
        start: u64,
        end: u64,
    ) -> Option<Recognition> {
        let (start_ms, end_ms) = (to_ms(start), to_ms(end));
        let short = (speech.len() as f32) < SHORT_SPEECH_SECS * SAMPLE_RATE as f32;
        if self.uses_context && short && !context.is_empty() {
            let gap = (CONTEXT_GAP_SECS * SAMPLE_RATE as f32) as usize;
            let mut input = Vec::with_capacity(context.len() + gap + speech.len());
            input.extend_from_slice(context);
            input.resize(context.len() + gap, 0.0);
            input.extend_from_slice(speech);
            let result = self.recognize(&input)?;
            let all = words(&result, 0, to_ms(input.len() as u64));
            // Without word timings the context's words cannot be told apart,
            // so the speech is recognised alone instead.
            if !all.is_empty() {
                let words = words_after_context(
                    all,
                    to_ms(context.len() as u64),
                    to_ms((context.len() + gap) as u64),
                    start_ms,
                    end_ms,
                );
                let text = words
                    .iter()
                    .map(|word| word.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                return Some(Recognition { text, words });
            }
        }
        let result = self.recognize(speech)?;
        Some(Recognition {
            text: result.text.trim().to_owned(),
            words: words(&result, start_ms, end_ms),
        })
    }

    /// Transcribes every speech segment the VAD has completed.
    fn drain(
        &self,
        vad: &VoiceActivityDetector,
        total_samples: Option<u64>,
        recent: &mut Recent,
        context: &mut Vec<f32>,
        on_event: &mut dyn FnMut(Event) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        while let Some(speech) = vad.front() {
            let start = speech.start().max(0) as u64;
            let end = start + speech.samples().len() as u64;
            let recognition = self.recognize_in_context(speech.samples(), context, start, end);
            let samples = speech.samples();
            let recognised = recognition.as_ref().is_some_and(|r| !r.text.is_empty());
            // Only speech that was recognised as words is worth its context.
            if recognised {
                let keep = ((CONTEXT_SECS * SAMPLE_RATE as f32) as usize).min(samples.len());
                context.clear();
                context.extend_from_slice(&samples[samples.len() - keep..]);
            }
            vad.pop();
            recent.discard_before(end);

            if let Some(recognition) = recognition.filter(|r| !r.text.is_empty()) {
                on_event(Event::Segment(Segment {
                    start_ms: to_ms(start),
                    end_ms: to_ms(end),
                    text: recognition.text,
                    words: recognition.words,
                }))?;
            }
            if let Some(total) = total_samples.filter(|&total| total > 0) {
                on_event(Event::Progress((end as f32 / total as f32).min(1.0)))?;
            }
        }
        ControlFlow::Continue(())
    }
}

impl Transcriber for SherpaTranscriber {
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
        let max_recent =
            (self.vad_config.silero_vad.max_speech_duration + 1.0) as usize * SAMPLE_RATE as usize;
        let mut recent = Recent::default();
        let mut context = Vec::new();
        let mut since_partial = 0;
        // The VAD runs on whole windows; samples left over from one chunk
        // wait for the next.
        let mut pending: Vec<f32> = Vec::new();
        while let Some(chunk) = audio.next_chunk() {
            pending.extend(chunk?);
            let whole = pending.len() / VAD_WINDOW * VAD_WINDOW;
            for window in pending[..whole].chunks(VAD_WINDOW) {
                vad.accept_waveform(window);
            }
            if self.partial_every.is_some() {
                recent.push(&pending[..whole], max_recent);
            }
            pending.drain(..whole);
            if self
                .drain(&vad, total_samples, &mut recent, &mut context, on_event)
                .is_break()
            {
                return Err(Error::Cancelled);
            }

            let Some(every) = self.partial_every else {
                continue;
            };
            since_partial += whole;
            if since_partial >= every && vad.detected() {
                since_partial = 0;
                let end = recent.start + recent.samples.len() as u64;
                let text = self
                    .recognize_in_context(&recent.samples, &context, recent.start, end)
                    .map(|recognition| recognition.text)
                    .unwrap_or_default();
                if !text.is_empty() && on_event(Event::Partial(text)).is_break() {
                    return Err(Error::Cancelled);
                }
            }
        }
        if !pending.is_empty() {
            vad.accept_waveform(&pending);
        }
        vad.flush();
        if self
            .drain(&vad, total_samples, &mut recent, &mut context, on_event)
            .is_break()
            || on_event(Event::Progress(1.0)).is_break()
        {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

/// The audio since the end of the last finished segment, which holds the
/// speech in progress for partial recognitions.
#[derive(Debug, Default)]
struct Recent {
    /// Position of `samples[0]` in the whole stream.
    start: u64,
    samples: Vec<f32>,
}

impl Recent {
    /// Appends `samples`, keeping at most the last `max` samples.
    fn push(&mut self, samples: &[f32], max: usize) {
        self.samples.extend_from_slice(samples);
        let excess = self.samples.len().saturating_sub(max);
        self.samples.drain(..excess);
        self.start += excess as u64;
    }

    fn discard_before(&mut self, position: u64) {
        let n = (position.saturating_sub(self.start) as usize).min(self.samples.len());
        self.samples.drain(..n);
        self.start += n as u64;
    }
}

/// Keeps the words of a recognition of context, gap and speech that start
/// after the context ends at `context_ms`, and moves them from input time,
/// where the speech starts at `speech_from_ms`, to stream time
/// `start_ms..end_ms`.
fn words_after_context(
    words: Vec<Word>,
    context_ms: u64,
    speech_from_ms: u64,
    start_ms: u64,
    end_ms: u64,
) -> Vec<Word> {
    let shift = |ms: u64| (start_ms + ms.saturating_sub(speech_from_ms)).min(end_ms);
    words
        .into_iter()
        .filter(|word| word.start_ms >= context_ms)
        .map(|word| Word {
            start_ms: shift(word.start_ms),
            end_ms: shift(word.end_ms),
            text: word.text,
        })
        .collect()
}

/// Text and word timings of one recognition, with times in milliseconds
/// from the start of the stream.
struct Recognition {
    text: String,
    words: Vec<Word>,
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
    fn drops_context_words_and_moves_the_rest_to_stream_time() {
        let word = |start_ms, end_ms, text: &str| Word {
            start_ms,
            end_ms,
            text: text.into(),
        };
        // 3 s of context, 0.2 s gap, then speech recognised at 50 s.
        let words = vec![
            word(2_500, 2_900, "tecnologia."),
            word(3_100, 3_300, "Olá,"),
            word(3_300, 3_700, "Porto."),
        ];

        let kept = words_after_context(words, 3_000, 3_200, 50_000, 50_400);

        assert_eq!(
            kept,
            [word(50_000, 50_100, "Olá,"), word(50_100, 50_400, "Porto.")]
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
        let transcriber =
            SherpaTranscriber::load(&ModelStore::new(&dir), &Engine::Parakeet, 2).unwrap();
        let mut segments = Vec::new();
        let mut last_progress = 0.0;

        transcriber
            .transcribe(&mut decoder, &mut |event| {
                match event {
                    Event::Segment(segment) => segments.push(segment),
                    Event::Progress(p) => last_progress = p,
                    Event::Partial(_) => {}
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

    /// Needs Whisper installed in `VERBATIM_MODELS_DIR` too.
    #[test]
    #[ignore = "needs installed models in VERBATIM_MODELS_DIR"]
    fn whisper_transcribes_speech_in_the_given_language() {
        let dir = std::env::var("VERBATIM_MODELS_DIR").expect("VERBATIM_MODELS_DIR is set");
        let audio = std::env::var("VERBATIM_TEST_AUDIO").expect("VERBATIM_TEST_AUDIO is set");
        verbatim_audio::init().unwrap();
        let mut decoder =
            verbatim_audio::Decoder::open(std::path::Path::new(&audio), SAMPLE_RATE).unwrap();
        let engine = Engine::Whisper {
            language: Some("en".into()),
        };
        let transcriber = SherpaTranscriber::load(&ModelStore::new(&dir), &engine, 2).unwrap();
        let mut segments = Vec::new();

        transcriber
            .transcribe(&mut decoder, &mut |event| {
                if let Event::Segment(segment) = event {
                    segments.push(segment);
                }
                ControlFlow::Continue(())
            })
            .unwrap();

        assert!(!segments.is_empty());
        assert!(
            segments
                .iter()
                .all(|s| !s.text.is_empty() && s.words.is_empty())
        );
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
        let transcriber =
            SherpaTranscriber::load(&ModelStore::new(&dir), &Engine::Parakeet, 2).unwrap();

        let result = transcriber.transcribe(&mut decoder, &mut |_| ControlFlow::Break(()));

        assert!(matches!(result, Err(Error::Cancelled)));
    }

    #[test]
    fn recent_keeps_only_the_latest_samples() {
        let mut recent = Recent::default();

        recent.push(&[1.0, 2.0, 3.0], 4);
        recent.push(&[4.0, 5.0], 4);

        assert_eq!(recent.samples, [2.0, 3.0, 4.0, 5.0]);
        assert_eq!(recent.start, 1);
    }

    #[test]
    fn recent_discards_audio_of_finished_segments() {
        let mut recent = Recent::default();
        recent.push(&[1.0, 2.0, 3.0, 4.0, 5.0], 10);

        recent.discard_before(3);
        recent.discard_before(1);

        assert_eq!(recent.samples, [4.0, 5.0]);
        assert_eq!(recent.start, 3);
        recent.discard_before(99);
        assert!(recent.samples.is_empty());
        assert_eq!(recent.start, 5);
    }

    #[test]
    #[ignore = "needs installed models in VERBATIM_MODELS_DIR"]
    fn sends_partials_before_each_segment() {
        let dir = std::env::var("VERBATIM_MODELS_DIR").expect("VERBATIM_MODELS_DIR is set");
        let audio = std::env::var("VERBATIM_TEST_AUDIO").expect("VERBATIM_TEST_AUDIO is set");
        verbatim_audio::init().unwrap();
        let mut decoder =
            verbatim_audio::Decoder::open(std::path::Path::new(&audio), SAMPLE_RATE).unwrap();
        let transcriber = SherpaTranscriber::load(&ModelStore::new(&dir), &Engine::Parakeet, 2)
            .unwrap()
            .with_partials(Duration::from_millis(500));
        let mut events = Vec::new();

        transcriber
            .transcribe(&mut decoder, &mut |event| {
                if !matches!(event, Event::Progress(_)) {
                    events.push(event);
                }
                ControlFlow::Continue(())
            })
            .unwrap();

        let partials = events
            .iter()
            .filter(|e| matches!(e, Event::Partial(_)))
            .count();
        let segments = events
            .iter()
            .filter(|e| matches!(e, Event::Segment(_)))
            .count();
        assert!(segments > 0);
        assert!(
            partials >= segments,
            "{partials} partials, {segments} segments"
        );
        assert!(matches!(events.first(), Some(Event::Partial(_))));
        eprintln!("{events:#?}");
    }

    #[test]
    fn rejects_missing_models() {
        let root = std::env::temp_dir().join("verbatim-no-models");

        let result = SherpaTranscriber::load(&ModelStore::new(root), &Engine::Parakeet, 1);

        assert!(matches!(result, Err(Error::ModelMissing(_))));
    }
}
