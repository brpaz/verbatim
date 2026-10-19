# Architecture

This document describes the architecture of the GTK application template.

## Overview

The template provides a modern GTK4 application using:
- **GTK4** and **libadwaita** with GObject subclassing
- **Blueprint** for UI definitions, compiled to GtkBuilder XML at build time
- **Meson** build system
- **Rust** with glib/gio bindings

## Project Structure

```
├── src/
│   ├── main.rs        # Entry point
│   ├── application.rs # Application subclass
│   ├── window.rs      # Main window subclass
│   └── config.rs      # Build-time configuration
├── data/
│   └── resources/
│       ├── ui/        # Blueprint (.blp) UI definitions
│       └── style.css # Application styles
├── meson.build        # Meson build definition
└── Cargo.toml        # Rust crate definition
```

## Application Model

### Entry Point (`main.rs`)

The entry point initializes:
1. **Logging** - `tracing_subscriber` for debug output
2. **Internationalization** - `gettextrs` for translations
3. **Resources** - GTK resource bundle loading
4. **Application instance** - Creates and runs the application

### Application Subclass

`application.rs` defines `{{ app_main_class }}Application`:

- Extends `adw::Application`
- Manages the main window lifecycle
- Sets up actions, accelerators, and CSS
- Provides the about dialog

```
{{ app_main_class }}Application
    ├── activate()      # Creates/shows main window on activation
    ├── startup()       # Sets up CSS, actions, accelerators
    └── run()           # Runs the application
```

### Window Subclass

`window.rs` defines `{{ app_main_class }}ApplicationWindow`:

- Extends `adw::ApplicationWindow`
- Uses `#[derive(gtk::CompositeTemplate)]` for UI binding
- Saves/restores window size via GSettings
- Supports devel profile CSS class

```
{{ app_main_class }}ApplicationWindow
    ├── new()           # Constructor
    ├── load_window_size()  # Restore from GSettings
    └── save_window_size() # Persist to GSettings
```

## Configuration

`config.rs` provides build-time configuration via the `config_var!` macro:

```rust
config_var!(APP_ID);      // Application ID
config_var!(PROFILE);    // Build profile (Devel/Release)
config_var!(VERSION);    // Version string
config_var!(LOCALEDIR); // Translation directory
config_var!(PKGDATADIR);// Data directory
```

These are set by Meson at compile time via `MESON_<VAR>` environment variables.

## UI Definition

Windows use [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/) templates (`data/resources/ui/*.blp`). Meson runs `blueprint-compiler batch-compile` and bundles the resulting GtkBuilder XML into the GResource:

```blueprint
template ${{ app_main_class }}ApplicationWindow: Adw.ApplicationWindow {
  content: Adw.ToolbarView {
    [top]
    Adw.HeaderBar headerbar {}

    content: /* Window content */;
  };
}
```

### Template Child Access

Child widgets are accessed via `#[template_child]`:

```rust
#[derive(Debug, gtk::CompositeTemplate)]
#[template(resource = "/com/example/myapp/ui/window.ui")]
pub struct MyAppWindow {
    #[template_child]
    pub headerbar: TemplateChild<gtk::HeaderBar>,
}
```

## Actions and Accelerators

### GTK Actions

Actions are defined using `gio::ActionEntry`:

```rust
let action_quit = gio::ActionEntry::builder("quit")
    .activate(|app, _, _| {
        app.quit();
    })
    .build();
self.add_action_entries([action_quit]);
```

### Accelerators

Keyboard shortcuts are set via `set_accels_for_action()`:

```rust
self.set_accels_for_action("app.quit", &["<Control>q"]);
self.set_accels_for_action("window.close", &["<Control>w"]);
```

## CSS Styling

CSS is loaded via `GtkCssProvider`:

```rust
fn setup_css(&self) {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/com/example/myapp/style.css");
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
```

## Internationalization

Translations use `gettextrs`:

```rust
use gettextrs::gettext;

// Mark strings for translation
let label = gettext("Hello world!");
```

Translation files (.po) are compiled to .mo files in `data/locale/`.

## Build System

### Meson Integration

Meson builds the project and generates:
- `gresource.xml` - GTK resource bundle manifest
- `config.rs` - Build variables injected at compile time
- Compiled translations (.mo files)

### Resource Compilation

UI files and CSS are compiled into a GTK resource bundle:
- Blueprint files are compiled to XML, then embedded in the binary
- No external UI files needed at runtime

## Extension Points

To extend the application:

1. **Add new windows** - Create new window.rs module + Blueprint file
2. **Add actions** - Add `gio::ActionEntry` in application.rs
3. **Add accelerators** - Add in `setup_accels()`
4. **Add CSS** - Extend style.css
5. **Add resources** - Add to meson.build

## Key Dependencies

| Package | Purpose |
|---------|----------|
| gtk | GTK4 bindings |
| adw (libadwaita) | libadwaita bindings |
| gio | GIO library |
| glib | GLib bindings |
| gettextrs | Internationalization |
| tracing | Logging |
| tracing-subscriber | Logging output |

## Runtime Flow

```
main()
  ├── Initialize tracing logger
  ├── Load translations (gettextrs)
  ├── Load GTK resources
  ├── Create Application instance
  └── Application::run()
      └── GtkApplication::activate()
          ├── Create main window
          └── Present window
```

## Data Flow

```
User Input
    │
    ▼
GTK Event (signal)
    │
    ├── Action (gio::Action)
    │   └── Handler closure
    │
    └── Subclass method
        └── Update model
            │
            ▼
        Notify property change
            │
            ▼
        Widget emits signal
            │
            ▼
        UI updates via binding
```