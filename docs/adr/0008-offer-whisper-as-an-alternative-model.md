# ADR 0008: Offer Whisper as an alternative model

## Status
Accepted

## Context
Parakeet (ADR 0003) cannot be told the language of the speech. It detects it from the audio, and takes short European Portuguese phrases for Spanish or English; recognising short speech after context (ADR 0007) fixes the language, but not every word. It also covers only 25 European languages.

OpenAI's Whisper takes the language as a parameter and covers about 100 languages. sherpa-onnx, which already runs Parakeet and the VAD, also runs Whisper, so it adds no native library.

## Decision
- Offer Whisper Large v3 Turbo (int8, from the sherpa-onnx exports) as a second model, chosen in Preferences, with Parakeet as the default.
- Offer a language setting with Whisper: detect automatically, or a fixed language such as Portuguese. Parakeet ignores it.
- Both models share the VAD, segmenting, partial results and events through one `SherpaTranscriber`, configured by an `Engine`.
- Re-decode live speech every 2 seconds with Whisper instead of every second, since each decode encodes a 30-second window whatever the speech length.
- The sherpa-onnx Whisper export gives no token timestamps, so subtitle cues estimate word timings within each segment, in proportion to word length. Context recognition (ADR 0007) is skipped for Whisper: it needs word timings, and a fixed language makes it unnecessary.

## Alternatives Considered
- **whisper.cpp (`whisper-rs`)**: Vulkan support and word timestamps, but a second native inference library to build and package next to ONNX Runtime.
- **Canary-1B v2**: the same 25 languages as Parakeet with a language parameter, but an ONNX export for sherpa-onnx could not be confirmed, and it is larger than Whisper Turbo.
- **Whisper small or medium**: smaller downloads, but noticeably less accurate in Portuguese; one Whisper option keeps the choice simple.

## Consequences
- Choosing Whisper downloads about 1 GB more, after asking.
- Whisper transcribes files several times faster than real time on a laptop CPU, but slower than Parakeet, and live text updates less often.
- Subtitle timings from Whisper are approximate within each segment; segment boundaries stay exact.
- Whisper can hallucinate text over noise. The VAD keeps silence out, as with Parakeet.
