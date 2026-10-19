# ADR 0001: Use libadwaita and Blueprint for the UI

## Status
Accepted

## Context
The application follows the GNOME Human Interface Guidelines and ships as a Flatpak on the GNOME runtime. Plain GTK4 widgets leave adaptive layouts, styling, the About, Preferences and Shortcuts dialogs and dark-mode handling to the application. UI definitions written as GtkBuilder XML are verbose and hard to review.

## Decision
- Build the UI on libadwaita (`adw::Application`, `adw::ApplicationWindow`, `Adw.ToolbarView`, `Adw.AboutDialog`, `Adw.PreferencesDialog`, `Adw.ShortcutsDialog`).
- Write UI definitions in [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/) (`data/resources/ui/*.blp`). Meson compiles them to GtkBuilder XML with `blueprint-compiler batch-compile` and bundles the result into the GResource.
- Require libadwaita 1.8 (for `Adw.ShortcutsDialog`) and the matching GTK 4.20.

## Alternatives Considered
- **Plain GTK4**: no extra dependency, but every dialog and adaptive behaviour is hand-built.
- **GtkBuilder XML**: no build-time compiler, but noisy diffs and no editor tooling beyond schema validation.
- **Older libadwaita with `Gtk.ShortcutsWindow`**: lower runtime floor, but the widget is deprecated in GTK 4.18.

## Consequences
- Distribution packages need libadwaita 1.8, which rules out `ubuntu-latest` runners. CI builds in Fedora and Debian unstable containers.
- `blueprint-compiler` is a build dependency of the project, the Flatpak manifests and CI.
- Each new `.blp` file is registered in `data/resources/meson.build`, `resources.gresource.xml` and `po/POTFILES.in`.
