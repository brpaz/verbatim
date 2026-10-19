# Adding a New Application Window

This guide explains how to add additional windows to your GTK application beyond the main window.

## Overview

In this template, windows are implemented as GObject subclasses with a corresponding GTK UI template file. The main window follows a specific pattern that you can replicate for additional windows.

## Step 1: Create the Window UI Definition

Create a new Blueprint file in `data/resources/ui/`, saved as `settings_window.blp`:

```blueprint
using Gtk 4.0;
using Adw 1;

template $MyAppSettingsWindow: Adw.Window {
  title: _("Settings");

  content: Adw.ToolbarView {
    [top]
    Adw.HeaderBar headerbar {}

    content: Label {
      label: _("Settings content here");
    };
  };
}
```

Register the file in three places:

- `data/resources/meson.build`: add `ui/settings_window.blp` to the `blueprints` target inputs.
- `data/resources/resources.gresource.xml`: add `<file compressed="true" preprocess="xml-stripblanks">ui/settings_window.ui</file>`. Blueprint compiles each `.blp` to a `.ui` of the same name.
- `po/POTFILES.in`: add `data/resources/ui/settings_window.blp` so its strings are translated.

## Step 2: Create the Window Rust Module

Create `src/window/settings.rs` (or add to an existing windows module):

```rust
use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/com/example/myapp/ui/settings_window.ui")]
    pub struct MyAppSettingsWindow {
        #[template_child]
        pub headerbar: TemplateChild<adw::HeaderBar>,
    }

    impl Default for MyAppSettingsWindow {
        fn default() -> Self {
            Self {
                headerbar: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MyAppSettingsWindow {
        const NAME: &'static str = "MyAppSettingsWindow";
        type Type = super::MyAppSettingsWindow;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MyAppSettingsWindow {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for MyAppSettingsWindow {}
    impl WindowImpl for MyAppSettingsWindow {}
    impl AdwWindowImpl for MyAppSettingsWindow {}
}

glib::wrapper! {
    pub struct MyAppSettingsWindow(ObjectSubclass<imp::MyAppSettingsWindow>)
        @extends gtk::Widget, gtk::Window, adw::Window,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl MyAppSettingsWindow {
    pub fn new(app: &MyAppApplication) -> Self {
        glib::Object::builder()
            .property("application", app)
            .property("title", "Settings")
            .build()
    }
}
```

## Step 3: Register the Module

Add the module to your crate's `lib.rs`:

```rust
mod window;
pub use window::MyAppSettingsWindow;
```

## Step 4: Add a Action to Open the Window

In `src/application.rs`, add a new action in `setup_gactions()`:

```rust
fn setup_gactions(&self) {
    // ... existing actions ...
    
    let action_settings = gio::ActionEntry::builder("settings")
        .activate(|app: &Self, _, _| {
            let window = MyAppSettingsWindow::new(app);
            window.set_transient_for(Some(&app.main_window()));
            window.present();
        })
        .build();
    
    self.add_action_entries([action_settings]);
}
```

Then add the accelerator in `setup_accels()`:

```rust
fn setup_accels(&self) {
    // ... existing accels ...
    self.set_accels_for_action("app.settings", &["<Control>,"]);
}
```

## Step 5: Add Menu Item (Optional)

Add a menu item in your Blueprint file to trigger the action:

```blueprint
item {
  label: _("_Settings");
  action: "app.settings";
}
```

## Key Concepts

### Template Child

The `#[template_child]` attribute gives you direct access to child widgets defined in your Blueprint file:

```rust
#[template_child]
pub headerbar: TemplateChild<adw::HeaderBar>,
```

Access it via `self.imp().headerbar.get()`.

### Window Parent Types

- `adw::Window` - Basic adaptive window
- `adw::ApplicationWindow` - Window tied to an application (handles app menu, etc.)
- `adw::Dialog` - Adaptive dialog presented over a parent window with `dialog.present(Some(&parent))`
- `gtk::Popover` - Popover window

### Modal Windows

Set a window as modal to block interaction with parent:

```rust
window.set_modal(true);
window.set_transient_for(Some(&parent_window));
```

## Complete Example

See the main window implementation in `src/window.rs` for a complete reference including:
- GSettings integration for persisting window size
- CSS class handling for devel profiles
- Window state persistence