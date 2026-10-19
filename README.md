# Verbatim

> Transcribe audio files and caption live audio with local speech-to-text models

[![CI](https://img.shields.io/github/actions/workflow/status/brpaz/verbatim/ci.yml?branch=main&label=CI&logo=github&style=for-the-badge)](https://github.com/brpaz/verbatim/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE.md)
[![Flatpak](https://img.shields.io/badge/flatpak-ready-4A90D9?style=for-the-badge&logo=flatpak&logoColor=white)](https://flatpak.org)

<div align="center">

![Main window](data/resources/screenshots/screenshot1.png "Main window")

</div>

## Features

- Transcribes audio and video files on your computer: nothing is uploaded
- 25 European languages with NVIDIA Parakeet TDT v3, or about 100 with OpenAI Whisper Large v3 Turbo, which can be told the language
- Shows the transcript as it is recognised, and plays the file from any line
- Captions live audio from the microphone or from whatever the computer plays
- Saves the transcript as text, or as SRT or WebVTT subtitles

The speech models (about 670 MB for Parakeet, 1 GB for Whisper) are downloaded once, on first use, after asking.

## Install

Download the package for your system from the [latest release](https://github.com/brpaz/verbatim/releases/latest):

```bash
# Flatpak
flatpak install --user dev.brunopaz.Verbatim.flatpak
flatpak run dev.brunopaz.Verbatim

# Debian/Ubuntu
sudo apt install ./verbatim.deb

# Fedora
sudo dnf install ./verbatim.rpm
```

## Build from Source

```bash
git clone https://github.com/brpaz/verbatim
cd verbatim
task run
```

Full prerequisites, running tests, building the Flatpak/`.deb`/`.rpm`, and the Taskfile reference: [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributors

<a href="https://github.com/brpaz/verbatim/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=brpaz/verbatim" alt="Contributors" />
</a>

## License

This repo is MIT licensed. See [LICENSE.md](LICENSE.md).
