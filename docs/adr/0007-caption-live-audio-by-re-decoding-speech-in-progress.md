# ADR 0007: Caption live audio by re-decoding the speech in progress

## Status
Accepted

## Context
Live captions should show words while they are spoken, from the microphone or from what the computer plays (videos, calls without subtitles). Parakeet TDT (ADR 0003) is an offline model: it transcribes a whole stretch of audio at once, and gives its best text only for a complete utterance. Waiting for each pause would leave a sentence invisible until it ends.

## Decision
- Capture with GStreamer's `pulsesrc`: the default input device for the microphone, or `@DEFAULT_MONITOR@` for system audio. The user picks one, and the choice is remembered.
- Feed the capture through the same VAD and Parakeet pipeline as files. Each VAD segment becomes a final caption line.
- While the VAD detects speech, re-decode the audio since the last final line every second, and show the result as a provisional line that the next result or the final line replaces.
- Cap live segments at 10 seconds, so long runs of speech become final text sooner and each re-decode stays short, and end a line only after half a second of silence.
- Recognise speech shorter than 6 seconds, final or partial, after the last 3 seconds of the previous recognised segment, and drop the words that fall in that context. Parakeet detects the language from the audio alone, and a word said alone ("Olá", "Porto") is often taken for English or Spanish. File transcription does the same for its short segments.
- Stopping ends the capture like the end of a file: speech already captured is still transcribed.

## Alternatives Considered
- **Streaming Zipformer transducer (sherpa-onnx online recogniser)**: word-by-word output with less compute, but English-only or single-language models, less accurate than Parakeet, and a second model to download.
- **Final lines only**: the least compute, but text lags speech by a whole sentence.

## Consequences
- Captions cover the same 25 languages as file transcription, with one set of models.
- Re-decoding costs CPU on top of the final recognition; with a 1-second interval it keeps up with real time on a laptop CPU. The context adds up to 3.2 seconds of audio to each short recognition.
- Context fixes the language of short speech, not the recognition of names or rare words, which would need a model that takes a language hint.
- Captured audio is not saved, so a live transcript cannot be played back. It can be copied or saved like a file transcript.
- The Flatpak needs the PulseAudio socket, which also grants microphone access.
