#!/bin/sh
# Regenerates build-aux/cargo-sources.json from Cargo.lock. Flatpak builds
# have no network access, so Cargo reads its crates from these sources.
# Requires curl and uv. Run from the repository root.
set -eu

# Commit of flatpak/flatpak-builder-tools providing flatpak-cargo-generator.
GENERATOR_REV=74697c75b630d7330e77250fc13cb5ea688d9479

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
curl -fsSL -o "$dir/flatpak-cargo-generator.py" \
  "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$GENERATOR_REV/cargo/flatpak-cargo-generator.py"
uv run --no-project --with tomlkit --with aiohttp \
  python -I "$dir/flatpak-cargo-generator.py" Cargo.lock -o build-aux/cargo-sources.json
