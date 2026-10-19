use gettextrs::gettext;
use tracing::{debug, info};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::config::{APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::window::VerbatimApplicationWindow;

mod imp {
    use super::*;
    use glib::WeakRef;
    use std::cell::OnceCell;

    #[derive(Debug, Default)]
    pub struct VerbatimApplication {
        pub window: OnceCell<WeakRef<VerbatimApplicationWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VerbatimApplication {
        const NAME: &'static str = "VerbatimApplication";
        type Type = super::VerbatimApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for VerbatimApplication {}

    impl ApplicationImpl for VerbatimApplication {
        fn activate(&self) {
            debug!("GtkApplication<VerbatimApplication>::activate");
            self.parent_activate();
            let app = self.obj();

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.present();
                return;
            }

            let window = VerbatimApplicationWindow::new(&app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.main_window().present();
        }

        fn open(&self, files: &[gio::File], _hint: &str) {
            self.activate();
            if let Some(file) = files.first() {
                self.obj().main_window().open_file(file.clone());
            }
        }

        fn startup(&self) {
            debug!("GtkApplication<VerbatimApplication>::startup");
            self.parent_startup();
            let app = self.obj();

            // Set icons for shell
            gtk::Window::set_default_icon_name(*APP_ID);

            app.setup_css();
            app.setup_gactions();
            app.setup_accels();
        }
    }

    impl GtkApplicationImpl for VerbatimApplication {}
    impl AdwApplicationImpl for VerbatimApplication {}
}

glib::wrapper! {
    pub struct VerbatimApplication(ObjectSubclass<imp::VerbatimApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl VerbatimApplication {
    fn main_window(&self) -> VerbatimApplicationWindow {
        self.imp().window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        // Quit
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| {
                // This is needed to trigger the delete event and saving the window state
                app.main_window().close();
                app.quit();
            })
            .build();

        // About
        let action_about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| {
                app.show_about_dialog();
            })
            .build();

        let action_preferences = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                crate::preferences::present(&app.main_window());
            })
            .build();
        self.add_action_entries([action_quit, action_about, action_preferences]);
    }

    // Sets up keyboard shortcuts
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("win.open", &["<Control>o"]);
        self.set_accels_for_action("app.preferences", &["<Control>comma"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.show-help-overlay", &["<Control>question"]);
    }

    fn setup_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/dev/brunopaz/Verbatim/style.css");
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn authors() -> Vec<&'static str> {
        // Authors are defined in Cargo.toml
        env!("CARGO_PKG_AUTHORS").split(":").collect()
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name(gettext("Verbatim"))
            .application_icon(*APP_ID)
            .developer_name("Bruno Paz")
            .website("https://github.com/brpaz/verbatim")
            .issue_url("https://github.com/brpaz/verbatim/issues")
            .license_type(gtk::License::MitX11)
            .version(*VERSION)
            .translator_credits(gettext("translator-credits"))
            .developers(Self::authors())
            .artists(vec!["Verbatim contributors"])
            .build();

        dialog.present(Some(&self.main_window()));
    }

    pub fn run(&self) -> glib::ExitCode {
        info!("{} ({})", "Verbatim", *APP_ID);
        info!("Version: {} ({})", *VERSION, *PROFILE);
        info!("Datadir: {}", *PKGDATADIR);

        ApplicationExtManual::run(self)
    }
}

impl Default for VerbatimApplication {
    fn default() -> Self {
        glib::Object::builder()
            .property("application-id", *APP_ID)
            .property("resource-base-path", "/dev/brunopaz/Verbatim/")
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }
}
