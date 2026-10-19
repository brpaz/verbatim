# ADR 0002: Build with Meson around Cargo

## Status
Accepted

## Context
A GNOME application installs more than a binary: a GResource bundle, GSettings schema, desktop file, AppStream metainfo, icons, translations and a D-Bus service file. Flatpak and distribution packagers expect `meson install`. Cargo alone does not provide these steps.

## Decision
- Meson is the top-level build system. A `custom_target` runs `cargo build` and installs the binary; build-time values reach Rust through `MESON_*` environment variables read in `src/config.rs`.
- Meson compiles Blueprint files and resources, merges translations into the desktop and metainfo files, compiles the schema and runs the desktop, metainfo and schema validators as tests.
- `cargo test` runs under `meson test` with the bundle path pointing at the build tree. Plain `cargo test` also works; tests that need the bundle return early.

## Alternatives Considered
- **Cargo with `build.rs` and `glib-build-tools`**: simpler loop for tiny apps, but no installation, translation merging or validation, and Flatpak packaging needs extra scripts.
- **Cargo plus a Makefile or Taskfile**: reimplements what Meson modules already provide.

## Consequences
- Contributors need Meson and Ninja besides the Rust toolchain; devenv provides both.
- `cargo` commands outside Meson need the `MESON_*` variables, otherwise reading a config value panics.
