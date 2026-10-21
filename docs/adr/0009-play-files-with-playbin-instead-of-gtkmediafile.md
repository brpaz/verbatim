# ADR 0009: Play files with playbin instead of GtkMediaFile

## Status
Accepted

## Context
The window plays the open file so that activating a transcript line can jump to it. `GtkMediaFile` plays through GStreamer's `playbin3`. In GStreamer 1.28, shipped by the GNOME 51 runtime, `decodebin3` inside `playbin3` aborts the process as an MP3 file loads, with `mq_slot_handle_stream_start: assertion failed: (collection)`. GStreamer 1.26 (GNOME 49) does not abort. The transcription pipeline uses `decodebin` and is not affected.

## Decision
- Play files with `Player`, a `GtkMediaStream` subclass in the app that drives a `playbin` (`decodebin` based) element.
- Decode audio only (`flags=audio`): the window has no video view, so video streams and cover art are skipped.
- Keep `GtkMediaControls` as the user interface; it accepts any `GtkMediaStream`.

## Alternatives Considered
- **Move back to the GNOME 49 runtime**: avoids the bug in the Flatpak only, ties the app to an older runtime, and leaves the .deb and .rpm packages crashing on distributions with GStreamer 1.28.
- **Wait for a GStreamer fix**: every MP3 crashes the app until then.

## Consequences
- The app depends on GStreamer directly, not only through `verbatim-audio`.
- About 150 lines of playback code to maintain: position updates, seeking, end of stream, errors and volume.
- Once a fixed GStreamer is in the supported runtimes and distributions, `GtkMediaFile` can replace `Player` again.
