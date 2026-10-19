# ADR 0003: Transcribe with Parakeet via sherpa-onnx

## Status
Accepted

## Context
Transcription must run fully locally, at acceptable speed on laptop CPUs, in at least English and Portuguese. It should not require a Python runtime inside the Flatpak. Subtitle export needs timings finer than whole utterances.

NVIDIA's Parakeet TDT models (the multilingual v3 covers 25 European languages) transcribe much faster than Whisper models of similar accuracy on CPU, and emit punctuation and token timestamps. sherpa-onnx (from the k2-fsa project) runs Parakeet through ONNX Runtime from a C API with official Rust bindings, and also provides Silero VAD and streaming models usable for live captions.

## Decision
- Use Parakeet TDT (multilingual v3, int8-quantized) as the speech recognition model.
- Run it through the official `sherpa-onnx` crate, with Silero VAD splitting the audio into speech segments of at most 20 seconds before recognition.
- Group Parakeet's subword tokens into words with start and end times, and build subtitle cues from those words.
- Hide the engine behind a `Transcriber` trait in `verbatim-core`, so the app does not depend on sherpa-onnx types.

## Alternatives Considered
- **Whisper via whisper.cpp (`whisper-rs`)**: covers about 100 languages and has a Vulkan backend for iGPUs, but runs much slower on CPU at comparable accuracy and can hallucinate text over silence. Kept as a possible second `Transcriber` implementation.
- **Vosk**: light and streams natively, but noticeably less accurate and without punctuation.
- **Wrapping an existing CLI (ownscribe, faster-whisper scripts)**: brings a Python runtime into the Flatpak, adds process startup per file and leaves only text output to parse.

## Consequences
- Languages outside Parakeet's set are unsupported until a second transcriber exists.
- sherpa-onnx's build script downloads prebuilt static libraries. The Flatpak manifests pin those archives by checksum, and their version must follow the `sherpa-onnx` crate version.
- The VAD keeps silence out of the recogniser, which cuts both CPU time and hallucinations, and its segments double as transcript lines.
