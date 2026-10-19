# ADR 0004: Copy the transcription crates from Parley

## Status
Accepted

## Context
[Parley](https://github.com/brpaz/parley), a local meeting recorder by the same author, already has a working model catalog with verified, resumable downloads, a GStreamer file decoder and a Parakeet transcriber. Verbatim needs the same pieces, with a different shape around them: no speaker tracks or diarization, but word timings, per-segment events and cancellation.

## Decision
- Copy Parley's `models`, `capture` (decoder only, as `verbatim-audio`) and `transcribe` crates into this workspace, and adapt them to Verbatim's needs.
- Extract a shared crate only once both apps' transcription APIs have settled.

## Alternatives Considered
- **Shared crate repository now**: one copy of the code, but every change in either app would first need a release of the shared crate, while both APIs are still changing.
- **Fresh implementation**: discards tested download, checksum and decoding code.

## Consequences
- Fixes to the copied code, such as the model catalog's pinned URLs and checksums, must be applied to both repositories until the extraction.
- The two copies may drift. The extraction should start from whichever API serves both apps.
