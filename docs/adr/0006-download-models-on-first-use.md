# ADR 0006: Download models on first use

## Status
Accepted

## Context
The VAD and Parakeet models together take about 670 MB. Bundling them in the Flatpak or distribution packages would make every app update re-download them, and would fix the model choice at build time.

## Decision
- Ship no models in the package. A model catalog compiled into the app lists each model's download URL, pinned to a commit, with its size, SHA-256 checksum and license.
- Before the first transcription, ask the user to download the missing models, showing their names, licenses and total size. Download them into the app's data directory, verify the checksums and show progress.
- Downloads resume after an interruption or cancellation, from the partial file left on disk.

## Alternatives Considered
- **Bundle models in the package**: works offline from install, but makes packages huge and couples model updates to app releases.
- **Flatpak extensions per model**: update cleanly through Flatpak, but don't help distribution packages, and need separate Flathub submissions per model.

## Consequences
- The first transcription needs a network connection, and the Flatpak needs network access.
- Changing the catalog requires an app release. Checksums protect against tampered or corrupted downloads.
- Each model's license must allow redistribution through its download source, and is shown to the user before downloading.
