# Contributing to Verbatim

This document outlines the guidelines and best practices for contributing to the project. Please take a moment to read through it before submitting a pull request.

## Table of Contents

- [Ways to Contribute](#ways-to-contribute)
- [Tech Stack](#tech-stack)
- [Setup the Development Environment](#setup-the-development-environment)
- [Development Workflow](#development-workflow)
- [Building Flatpak](#building-flatpak)
- [Available Tasks](#available-tasks)
- [Code Quality Tools](#code-quality-tools)
- [Conventional Commits](#conventional-commits)
- [Pull Request Guidelines](#pull-request-guidelines)
- [Review Process](#review-process)
- [Release Process](#release-process)

## Ways to Contribute

Contributions can take many forms, not just code. Here are some ways you can contribute:
- Reporting bugs or suggesting features by opening issues.
- Submitting pull requests with bug fixes or new features.
- Improving documentation.
- Reviewing and providing feedback on existing pull requests.
- Helping other users in the community.

## Tech Stack

This project uses the following technologies:

- **Language**: [Rust](https://www.rust-lang.org/)
- **GUI Framework**: [GTK4](https://gtk.org/) with [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/) (1.8 or newer)
- **UI Definitions**: [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/)
- **Build System**: [Meson](https://mesonbuild.com/) with [Ninja](https://ninja-build.org/)
- **Development Environment**: [Devenv](https://devenv.sh/) (Nix-based)
- **Task Runner**: [Task](https://taskfile.dev/) (aka go-task)
- **Git Hooks**: [Lefthook](https://github.com/evilmartians/lefthook)
- **Commit Linting**: [commitlint-rs](https://keisukeyamashita.github.io/commitlint-rs/)

## Setup the Development Environment

### Pre-requisites

This project uses [Devenv](https://devenv.sh/) to provide a self-contained development environment using the power of [Nix](https://nixos.org/).

While you could install the dependencies directly on your system if you like, using Devenv is the recommended away for the best possible experience.

To install Devenv, follow the instructions at [devenv.sh](https://devenv.sh/getting-started/).

[Direnv](https://direnv.net/) is also recommended. Direnv allows to automatically load the Devenv environment when you `cd` into the project directory, as well as simplify the management of project level environment variables. Follow the instructions at [direnv.net](https://direnv.net/docs/installation.html) to install Direnv and to integrate it with your shell.

#### Without Devenv

If you'd rather install dependencies directly, `devenv.nix` lists everything you need; the equivalent per-distro commands are:

```bash
# Fedora
sudo dnf install meson ninja-build gcc gcc-c++ pkgconf-pkg-config gtk4-devel libadwaita-devel blueprint-compiler glib2-devel gettext desktop-file-utils appstream flatpak-builder

# Debian/Ubuntu
sudo apt install meson ninja-build gcc g++ pkg-config libgtk-4-dev libadwaita-1-dev blueprint-compiler libglib2.0-dev gettext desktop-file-utils appstream flatpak-builder

# Arch
sudo pacman -S meson ninja gcc pkgconf gtk4 libadwaita blueprint-compiler glib2 gettext desktop-file-utils appstream flatpak-builder
```

Plus [Task](https://taskfile.dev/), [Lefthook](https://lefthook.dev/), [commitlint-rs](https://github.com/KeisukeYamashita/commitlint-rs) (`cargo install commitlint-rs`) for the workflow described below, and [nfpm](https://nfpm.goreleaser.com/) if you want to build `.deb`/`.rpm` packages locally (see [Building Native Packages](#building-native-packages)).

### Running the Development Environment

```bash
git clone https://github.com/brpaz/verbatim
cd verbatim
direnv allow
```

A devenv shell should be automatically loaded, after running `direnv allow`.

You can verify that you are in the Devenv shell by checking the output of `which cargo`. If you see a path like `/nix/store/.../bin/cargo`, then you are in the Devenv shell.

If you are not using Direnv or if your shell was not automatically loaded for some reason, you can manually start the Devenv shell by running:

```bash
devenv shell
```

## Development Workflow

After setting up the development environment, you can use the following commands to work on the project:

### Building the Project

To compile the project, run:

```bash
task build
```

This will configure the build with Meson and compile the project using Ninja. The compiled binary will be placed in the `_build/` directory.

### Running the Application

To run the application in development mode:

```bash
task run
```

### Running Tests

To run the test suite:

```bash
task test
```

This command runs `meson test` to execute all tests via the Meson test framework.

Tests that run the real speech models are ignored by default. To run them, point them at an installed model store and a file with English speech:

```bash
VERBATIM_MODELS_DIR=~/.local/share/verbatim/models \
VERBATIM_TEST_AUDIO=/path/to/speech.wav \
  cargo test -p verbatim-transcribe -- --include-ignored
```

The Whisper test also needs Whisper in that model store; choose Whisper in Preferences and transcribe once to download it.

`cargo test -p verbatim-audio -- --include-ignored` also captures briefly from the default microphone and system audio, and needs a running PipeWire or PulseAudio server.

### Cleaning Build Artifacts

To clean all build artifacts:

```bash
task clean
```

This removes the `_build/` and `_build-release/` directories.

## Code Quality Tools

This project uses several tools to ensure code quality and consistency:

- [Clippy](https://doc.rust-lang.org/clippy/) for linting Rust code.
- [rustfmt](https://rustformatter.sourceforge.net/) for code formatting.
- [Cargo](https://doc.rust-lang.org/cargo/) for testing and package management.
- [commitlint-rs](https://keisukeyamashita.github.io/commitlint-rs/) for enforcing commit message conventions.
- [Lefthook](https://github.com/evilmartians/lefthook) for managing Git hooks.


### Git Commit Hooks

This project uses [Lefthook](https://github.com/evilmartians/lefthook) to manage Git hooks. Lefthook hooks are automatically installed when you start the Devenv shell (via `lefthook install` in the `enterShell` hook).

### Running Additional Code Quality Checks

You can also run the following code quality checks:

```bash
# Run Clippy lints
task clippy

# Check code formatting
task fmt:check
```

For Rust projects, it is recommended to run Clippy and check formatting before submitting a pull request.

### Building Flatpak

To build a Flatpak locally for testing:

```bash
# Install Flatpak dependencies (one-time setup)
task flatpak:deps

# Build the Flatpak
task flatpak:build

# Build and install locally
task flatpak:install

# Run the installed Flatpak
task flatpak:run

```

### Building Native Packages

Besides the Flatpak, the CI also builds `.deb` and `.rpm` packages with [nfpm](https://nfpm.goreleaser.com/) (config in `nfpm.yaml`) from a `DESTDIR`-staged Meson install — no bundled dependencies, unlike the Flatpak; the package declares GTK4/libadwaita/GLib as regular system dependencies. To build one locally (requires `nfpm` on `$PATH`):

```bash
task package:deb   # or package:rpm
```

### Available Tasks

This project uses [Task](https://taskfile.dev/) for common development tasks. Run `task --list` to see all available tasks, or use these commonly:

| Task | Description |
|------|-------------|
| `task setup` | Configure Meson build (development profile) |
| `task build` | Build the project |
| `task run` | Build and run the application |
| `task install` / `task install:release` | Install the development build (from `_build/`) or the release build (from `_build-release/`); both install `/usr/bin/verbatim`, so uninstall one before installing the other |
| `task test` | Run all tests |
| `task test:cargo` | Run Cargo tests directly |
| `task clippy` | Run Clippy lints |
| `task fmt:check` | Check code formatting |
| `task blueprint:format` | Format Blueprint UI files |
| `task deny` | Check dependencies for advisories, licenses and sources (cargo-deny) |
| `task secrets` | Scan the repository for leaked secrets (gitleaks) |
| `task clean` | Remove build artifacts |
| `task flatpak:build` | Build a Flatpak |
| `task flatpak:install` | Install Flatpak locally |
| `task flatpak:bundle` | Create a .flatpak bundle file |
| `task package:deb` / `task package:rpm` | Build a `.deb` / `.rpm` with nfpm |
| `task dist` | Create a release tarball |

## Conventional Commits

This project follows the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) specification for commit messages. This helps to keep a consistent commit history and makes it easier to generate changelogs.

When writing commit messages, please use the following format:

```
<type>([optional scope]): <description>
```

Where `<type>` is one of the following:
- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation changes
- `ci`: Changes to CI configuration files and scripts
- `refactor`: Code changes that neither fix a bug nor add a feature
- `test`: Adding or updating tests
- `chore`: Changes to the build process or auxiliary tools and libraries.

[Commitlint](https://keisukeyamashita.github.io/commitlint-rs/) is used to enforce these rules on commit messages.

We know that sometimes during development, it can be cumbersome to follow these rules strictly. That´s why we only enforce commit message linting on `git push` operations, via the `pre-push` Git hook. This way, you can make as many local commits as you want, and only need to ensure that the commit messages are valid when pushing your changes to the remote repository.

## Pull Request Guidelines

When submitting a pull request, please ensure that:
- Your code follows the project's coding style and conventions.
- You have added tests for any new functionality or bug fixes.
- All tests pass.
- Your pull request description clearly explains the changes you have made and the reasons for them.
- Your branch is up to date with the main branch.
- Your commit messages follow the Conventional Commits specification.
- You have linked any related issues in the pull request description.
- You have requested reviews from relevant team members.
- You have addressed any feedback provided during the review process.
- You have squashed your commits into logical units, if necessary.

## Review Process

All pull requests will be reviewed by at least one other team member. The reviewer will check for code quality, adherence to project guidelines, and overall functionality. They may request changes or provide feedback before the pull request can be merged.

## Release Process

Releases are created using GitHub Releases. A draft release is kept up to date automatically by [draftsman](https://brpaz.github.io/draftsman/) (config in `.draftsman.yml`), which categorizes changes by [Conventional Commit](https://www.conventionalcommits.org/) type — see [Conventional Commits](#conventional-commits) below. Publishing the draft triggers the Flatpak/`.deb`/`.rpm` release builds.
