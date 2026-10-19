use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib};
use verbatim_core::{Format, Segment, clock_time, render};

use crate::application::VerbatimApplication;
use crate::config::{APP_ID, PROFILE};
use crate::transcription::{self, Job, Update};

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Verbatim/ui/window.ui")]
    pub struct VerbatimApplicationWindow {
        #[template_child]
        pub headerbar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub export_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub progress_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub progress_bar: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub segment_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub media_bar: TemplateChild<gtk::ActionBar>,
        #[template_child]
        pub media_controls: TemplateChild<gtk::MediaControls>,
        pub settings: gio::Settings,
        pub file: RefCell<Option<gio::File>>,
        pub media: RefCell<Option<gtk::MediaFile>>,
        pub segments: RefCell<Vec<Segment>>,
        pub job: RefCell<Option<Job>>,
        /// Identifies the current job, so updates from a replaced job are
        /// ignored.
        pub job_id: Cell<u64>,
        pub models_approved: Cell<bool>,
    }

    impl Default for VerbatimApplicationWindow {
        fn default() -> Self {
            Self {
                headerbar: TemplateChild::default(),
                toast_overlay: TemplateChild::default(),
                window_title: TemplateChild::default(),
                export_button: TemplateChild::default(),
                progress_banner: TemplateChild::default(),
                progress_bar: TemplateChild::default(),
                stack: TemplateChild::default(),
                segment_list: TemplateChild::default(),
                media_bar: TemplateChild::default(),
                media_controls: TemplateChild::default(),
                settings: gio::Settings::new(*APP_ID),
                file: RefCell::default(),
                media: RefCell::default(),
                segments: RefCell::default(),
                job: RefCell::default(),
                job_id: Cell::default(),
                models_approved: Cell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VerbatimApplicationWindow {
        const NAME: &'static str = "VerbatimApplicationWindow";
        type Type = super::VerbatimApplicationWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        // You must call `Widget`'s `init_template()` within `instance_init()`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VerbatimApplicationWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel Profile
            if *PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            // Load latest window state
            obj.load_window_size();

            obj.setup_gactions();
            obj.setup_drop_target();
            obj.setup_segment_list();
            obj.set_transcript_actions_enabled(false);
        }

        fn dispose(&self) {
            if let Some(job) = self.job.take() {
                job.cancel();
            }
        }
    }

    impl WidgetImpl for VerbatimApplicationWindow {}
    impl WindowImpl for VerbatimApplicationWindow {
        // Save window state on delete event
        fn close_request(&self) -> glib::Propagation {
            if let Err(err) = self.obj().save_window_size() {
                tracing::warn!("Failed to save window state, {}", &err);
            }

            // Pass close request on to the parent
            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for VerbatimApplicationWindow {}
    impl AdwApplicationWindowImpl for VerbatimApplicationWindow {}
}

glib::wrapper! {
    pub struct VerbatimApplicationWindow(ObjectSubclass<imp::VerbatimApplicationWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup,
                    gtk::Root, gtk::Native, gtk::ShortcutManager,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl VerbatimApplicationWindow {
    pub fn new(app: &VerbatimApplication) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn setup_gactions(&self) {
        let action_shortcuts = gio::ActionEntry::builder("show-help-overlay")
            .activate(|window: &Self, _, _| window.show_shortcuts())
            .build();
        let action_open = gio::ActionEntry::builder("open")
            .activate(|window: &Self, _, _| window.choose_file())
            .build();
        let action_export = gio::ActionEntry::builder("export")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|window: &Self, _, param| {
                let format = match param.and_then(|p| p.str()) {
                    Some("srt") => Format::Srt,
                    Some("vtt") => Format::WebVtt,
                    _ => Format::Text,
                };
                window.export(format);
            })
            .build();
        let action_copy = gio::ActionEntry::builder("copy")
            .activate(|window: &Self, _, _| window.copy_text())
            .build();
        let action_cancel = gio::ActionEntry::builder("cancel")
            .activate(|window: &Self, _, _| {
                if let Some(job) = window.imp().job.borrow().as_ref() {
                    job.cancel();
                }
            })
            .build();
        self.add_action_entries([
            action_shortcuts,
            action_open,
            action_export,
            action_copy,
            action_cancel,
        ]);
    }

    fn setup_drop_target(&self) {
        let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        target.connect_drop(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| {
                let Some(file) = value
                    .get::<gdk::FileList>()
                    .ok()
                    .and_then(|files| files.files().into_iter().next())
                else {
                    return false;
                };
                window.open_file(file);
                true
            }
        ));
        self.add_controller(target);
    }

    fn setup_segment_list(&self) {
        self.imp().segment_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, row| {
                let imp = window.imp();
                let Some(segment) = imp.segments.borrow().get(row.index() as usize).cloned() else {
                    return;
                };
                if let Some(media) = imp.media.borrow().as_ref() {
                    media.seek(segment.start_ms as i64 * 1000);
                    media.play();
                }
            }
        ));
    }

    fn choose_file(&self) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Audio and Video")));
        filter.add_mime_type("audio/*");
        filter.add_mime_type("video/*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open File"))
            .filters(&filters)
            .modal(true)
            .build();
        dialog.open(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |result| {
                    if let Ok(file) = result {
                        window.open_file(file);
                    }
                }
            ),
        );
    }

    /// Shows `file` and transcribes it, replacing any previous file.
    pub fn open_file(&self, file: gio::File) {
        let imp = self.imp();
        let Some(path) = file.path() else {
            self.toast(&gettext("Only files on this computer can be transcribed"));
            return;
        };
        if let Some(job) = imp.job.take() {
            job.cancel();
        }

        let name = file
            .basename()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        imp.window_title.set_subtitle(&name);
        imp.segments.borrow_mut().clear();
        imp.segment_list.remove_all();
        self.set_transcript_actions_enabled(false);
        self.set_media(&file);
        imp.file.replace(Some(file));
        imp.stack.set_visible_child_name("transcript");

        let missing = transcription::missing_models(&transcription::model_store());
        if missing.is_empty() || imp.models_approved.get() {
            self.start_job(path);
        } else {
            self.ask_model_download(path, &missing);
        }
    }

    fn set_media(&self, file: &gio::File) {
        let imp = self.imp();
        let media = gtk::MediaFile::for_file(file);
        media.connect_timestamp_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |media| window.highlight_segment_at((media.timestamp() / 1000) as u64)
        ));
        imp.media_controls.set_media_stream(Some(&media));
        imp.media_bar.set_revealed(true);
        if let Some(old) = imp.media.replace(Some(media)) {
            old.pause();
        }
    }

    /// Selects the segment playing at `ms`, keeping the previous one
    /// selected during pauses between segments.
    fn highlight_segment_at(&self, ms: u64) {
        let imp = self.imp();
        let index = imp
            .segments
            .borrow()
            .partition_point(|segment| segment.start_ms <= ms);
        let Some(row) = index
            .checked_sub(1)
            .and_then(|i| imp.segment_list.row_at_index(i as i32))
        else {
            return;
        };
        if !row.is_selected() {
            imp.segment_list.select_row(Some(&row));
        }
    }

    fn ask_model_download(&self, path: std::path::PathBuf, missing: &[&verbatim_models::Model]) {
        let size = missing.iter().map(|model| model.size()).sum();
        let models = missing
            .iter()
            .map(|model| format!("• {} ({})", model.name, model.license))
            .collect::<Vec<_>>()
            .join("\n");
        let body = gettext(
            "Speech is recognised on this computer. This needs AI models, downloaded once ({size}):",
        )
        .replace("{size}", &glib::format_size(size))
            + "\n\n"
            + &models;

        let dialog = adw::AlertDialog::new(Some(&gettext("Download AI Models?")), Some(&body));
        dialog.add_responses(&[
            ("later", &gettext("_Not Now")),
            ("download", &gettext("_Download")),
        ]);
        dialog.set_response_appearance("download", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("download"));
        dialog.set_close_response("later");
        dialog.connect_response(
            None,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_, response| {
                    if response == "download" {
                        window.imp().models_approved.set(true);
                        window.start_job(path.clone());
                    } else {
                        window.close_file();
                    }
                }
            ),
        );
        dialog.present(Some(self));
    }

    fn start_job(&self, path: std::path::PathBuf) {
        let imp = self.imp();
        let id = imp.job_id.get() + 1;
        imp.job_id.set(id);

        let (sender, receiver) = async_channel::bounded(64);
        imp.job.replace(Some(Job::start(path, sender)));
        self.show_progress(&gettext("Transcribing…"), 0.0);
        self.action_set_enabled("win.cancel", true);

        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                while let Ok(update) = receiver.recv().await {
                    if window.imp().job_id.get() != id {
                        break;
                    }
                    window.apply(update);
                }
            }
        ));
    }

    fn apply(&self, update: Update) {
        let imp = self.imp();
        match update {
            Update::Downloading(fraction) => {
                self.show_progress(&gettext("Downloading AI models…"), fraction)
            }
            Update::Transcribing(fraction) => {
                self.show_progress(&gettext("Transcribing…"), fraction)
            }
            Update::Segment(segment) => {
                imp.segment_list.append(&segment_row(&segment));
                imp.segments.borrow_mut().push(segment);
            }
            Update::Finished => {
                self.finish_job();
                if imp.segments.borrow().is_empty() {
                    imp.stack.set_visible_child_name("no-speech");
                }
            }
            Update::Cancelled => {
                self.finish_job();
                self.toast(&gettext("Transcription cancelled"));
            }
            Update::Failed(message) => {
                self.finish_job();
                self.toast(&gettext("Could not transcribe: {error}").replace("{error}", &message));
            }
        }
    }

    /// Hides the progress of the job that just ended. A partial transcript
    /// stays available.
    fn finish_job(&self) {
        let imp = self.imp();
        imp.job.take();
        imp.progress_banner.set_revealed(false);
        imp.progress_bar.set_visible(false);
        self.action_set_enabled("win.cancel", false);
        let has_segments = !imp.segments.borrow().is_empty();
        self.set_transcript_actions_enabled(has_segments);
        if !has_segments && imp.stack.visible_child_name().as_deref() == Some("transcript") {
            self.close_file();
        }
    }

    /// Returns to the initial page.
    fn close_file(&self) {
        let imp = self.imp();
        if let Some(media) = imp.media.take() {
            media.pause();
        }
        imp.media_controls.set_media_stream(gtk::MediaStream::NONE);
        imp.media_bar.set_revealed(false);
        imp.file.take();
        imp.window_title.set_subtitle("");
        imp.stack.set_visible_child_name("empty");
    }

    fn show_progress(&self, label: &str, fraction: f32) {
        let imp = self.imp();
        imp.progress_banner
            .set_title(&format!("{label} {:.0}%", fraction * 100.0));
        imp.progress_banner.set_revealed(true);
        imp.progress_bar.set_fraction(f64::from(fraction));
        imp.progress_bar.set_visible(true);
    }

    fn set_transcript_actions_enabled(&self, enabled: bool) {
        self.action_set_enabled("win.export", enabled);
        self.action_set_enabled("win.copy", enabled);
        self.imp().export_button.set_visible(enabled);
    }

    fn export(&self, format: Format) {
        let imp = self.imp();
        let stem = imp
            .file
            .borrow()
            .as_ref()
            .and_then(|file| file.path())
            .and_then(|path| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| gettext("Transcript"));
        let contents = render(&imp.segments.borrow(), format);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save Transcript"))
            .initial_name(format!("{stem}.{}", format.extension()))
            .modal(true)
            .build();
        dialog.save(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |result| {
                    let Ok(file) = result else {
                        return;
                    };
                    glib::spawn_future_local(async move {
                        let result = file
                            .replace_contents_future(
                                contents.into_bytes(),
                                None,
                                false,
                                gio::FileCreateFlags::REPLACE_DESTINATION,
                            )
                            .await;
                        match result {
                            Ok(_) => window.toast(&gettext("Transcript saved")),
                            Err((_, err)) => {
                                tracing::warn!(error = %err, "could not save transcript");
                                window.toast(&gettext("Could not save the transcript"));
                            }
                        }
                    });
                }
            ),
        );
    }

    fn copy_text(&self) {
        let text = render(&self.imp().segments.borrow(), Format::Text);
        self.clipboard().set_text(&text);
        self.toast(&gettext("Transcript copied"));
    }

    fn toast(&self, message: &str) {
        self.imp().toast_overlay.add_toast(adw::Toast::new(message));
    }

    fn show_shortcuts(&self) {
        let builder = gtk::Builder::from_resource("/dev/brunopaz/Verbatim/ui/shortcuts.ui");
        let dialog: adw::ShortcutsDialog = builder
            .object("shortcuts_dialog")
            .expect("shortcuts_dialog is defined in shortcuts.blp");
        dialog.present(Some(self));
    }

    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let imp = self.imp();

        let (width, height) = self.default_size();

        imp.settings.set_int("window-width", width)?;
        imp.settings.set_int("window-height", height)?;

        imp.settings
            .set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let imp = self.imp();

        let width = imp.settings.int("window-width");
        let height = imp.settings.int("window-height");
        let is_maximized = imp.settings.boolean("is-maximized");

        self.set_default_size(width, height);

        if is_maximized {
            self.maximize();
        }
    }
}

/// A transcript line: its start time, then its text.
fn segment_row(segment: &Segment) -> gtk::ListBoxRow {
    let time = gtk::Label::builder()
        .label(clock_time(segment.start_ms))
        .valign(gtk::Align::Start)
        .xalign(1.0)
        .width_chars(7)
        .css_classes(["dim-label", "numeric"])
        .build();
    let text = gtk::Label::builder()
        .label(&segment.text)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .hexpand(true)
        .build();
    let content = gtk::Box::builder()
        .spacing(12)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&time);
    content.append(&text);
    gtk::ListBoxRow::builder()
        .child(&content)
        .tooltip_text(gettext("Play from here"))
        .build()
}
