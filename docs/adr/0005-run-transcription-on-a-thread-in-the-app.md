# ADR 0005: Run transcription on a thread in the app process

## Status
Accepted

## Context
Transcription takes from seconds to many minutes and loads about 1.7 GB of model memory. It must not block the GTK main loop, and the transcript should appear while recognition runs. Live captions will need recognised text within a fraction of a second of speech.

Parley runs inference in a separate worker process, to keep a recording in progress safe from native crashes. Verbatim records nothing that a crash could lose: the input file stays on disk.

## Decision
- Run each transcription on a background thread inside the app process.
- The thread sends updates (download progress, transcription progress, each segment, then exactly one of finished, cancelled or failed) over an `async-channel` to the main loop.
- Cancellation is a shared flag, checked between VAD windows, segments and download chunks.
- Models are loaded per job and released when it ends.

## Alternatives Considered
- **Worker subprocess with JSON over pipes (as in Parley)**: survives native crashes and frees memory with the process, but adds a second binary, an IPC protocol and per-segment serialization, and adds latency for live captions.
- **D-Bus service**: heavier still, and nothing else needs to talk to the transcriber.

## Consequences
- A crash inside ONNX Runtime or GStreamer takes down the app. The user can reopen the file.
- Segments reach the UI as plain Rust values, with no protocol to version.
- Reloading the models takes a second or two per job, which is small next to transcription time.
