mod application;
mod config;
mod window;

use std::path::PathBuf;

use gettextrs::{LocaleCategory, gettext};
use gtk::{gio, glib};

use self::application::VerbatimApplication;
use self::config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};

fn main() -> glib::ExitCode {
    // Initialize logger
    tracing_subscriber::fmt::init();

    // Prepare i18n
    // SAFETY: runs first in `main`, before any other thread exists.
    unsafe { gettextrs::setlocale(LocaleCategory::LcAll, "") };
    gettextrs::bindtextdomain(*GETTEXT_PACKAGE, *LOCALEDIR)
        .expect("Unable to bind the text domain");
    gettextrs::textdomain(*GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&gettext("Verbatim"));

    let resources_file = std::env::var_os("MESON_RESOURCES_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(*RESOURCES_FILE));
    let res = gio::Resource::load(&resources_file).unwrap_or_else(|err| {
        panic!(
            "Could not load gresource file {}: {err}",
            resources_file.display()
        )
    });
    gio::resources_register(&res);

    let app = VerbatimApplication::default();
    app.run()
}
#[cfg(test)]
mod tests {
    use super::*;

    const UI_FILES: [&str; 3] = ["preferences.ui", "shortcuts.ui", "window.ui"];

    /// The bundle path is only known at runtime under `meson test`; plain
    /// `cargo test` has nothing to check and returns early.
    #[test]
    fn resource_bundle_contains_ui_files() {
        let Some(path) = std::env::var_os("MESON_RESOURCES_FILE") else {
            return;
        };
        let res = gio::Resource::load(&path).expect("resource bundle should load");

        for file in UI_FILES {
            let resource_path = format!("/dev/brunopaz/Verbatim/ui/{file}");
            res.lookup_data(&resource_path, gio::ResourceLookupFlags::NONE)
                .unwrap_or_else(|err| panic!("{resource_path} missing from bundle: {err}"));
        }
    }
}
