//! The Preferences dialog: speech recognition model and language.

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::APP_ID;
use crate::transcription::Engine;

/// `speech-model` setting values, in the order of the model row.
const MODELS: [&str; 2] = ["parakeet", "whisper"];

/// Languages offered for Whisper, by ISO 639-1 code. The empty code lets
/// Whisper detect the language.
fn languages() -> [(&'static str, String); 16] {
    [
        ("", gettext("Detect Automatically")),
        ("pt", gettext("Portuguese")),
        ("en", gettext("English")),
        ("es", gettext("Spanish")),
        ("fr", gettext("French")),
        ("de", gettext("German")),
        ("it", gettext("Italian")),
        ("nl", gettext("Dutch")),
        ("pl", gettext("Polish")),
        ("uk", gettext("Ukrainian")),
        ("ru", gettext("Russian")),
        ("tr", gettext("Turkish")),
        ("ar", gettext("Arabic")),
        ("hi", gettext("Hindi")),
        ("zh", gettext("Chinese")),
        ("ja", gettext("Japanese")),
    ]
}

pub fn present(parent: &impl IsA<gtk::Widget>) {
    let builder = gtk::Builder::from_resource("/dev/brunopaz/Verbatim/ui/preferences.ui");
    let dialog: adw::PreferencesDialog = builder
        .object("preferences_dialog")
        .expect("preferences_dialog is defined in preferences.blp");
    let model_row: adw::ComboRow = builder.object("model_row").expect("model_row is defined");
    let language_row: adw::ComboRow = builder
        .object("language_row")
        .expect("language_row is defined");
    let settings = gio::Settings::new(*APP_ID);

    model_row.set_model(Some(&gtk::StringList::new(&[
        &gettext("Parakeet TDT v3"),
        &gettext("Whisper Large v3 Turbo"),
    ])));
    let model = MODELS
        .iter()
        .position(|&id| id == settings.string("speech-model"))
        .unwrap_or(0);
    model_row.set_selected(model as u32);

    let languages = languages();
    let names: Vec<&str> = languages.iter().map(|(_, name)| name.as_str()).collect();
    language_row.set_model(Some(&gtk::StringList::new(&names)));
    let language = languages
        .iter()
        .position(|(code, _)| *code == settings.string("language"))
        .unwrap_or(0);
    language_row.set_selected(language as u32);

    update_rows(&model_row, &language_row);
    model_row.connect_selected_notify(glib::clone!(
        #[strong]
        settings,
        #[weak]
        language_row,
        move |row| {
            let id = MODELS
                .get(row.selected() as usize)
                .copied()
                .unwrap_or(MODELS[0]);
            if let Err(err) = settings.set_string("speech-model", id) {
                tracing::warn!(error = %err, "could not save the speech model");
            }
            update_rows(row, &language_row);
        }
    ));
    language_row.connect_selected_notify(move |row| {
        let code = languages
            .get(row.selected() as usize)
            .map_or("", |(code, _)| code);
        if let Err(err) = settings.set_string("language", code) {
            tracing::warn!(error = %err, "could not save the language");
        }
    });

    dialog.present(Some(parent));
}

/// Describes the selected model, and offers a language only to Whisper,
/// since Parakeet cannot be told one.
fn update_rows(model_row: &adw::ComboRow, language_row: &adw::ComboRow) {
    let whisper = model_row.selected() == 1;
    let (description, engine) = if whisper {
        (
            gettext("About 100 languages, slower. Models: {size}."),
            Engine::Whisper { language: None },
        )
    } else {
        (
            gettext("Fast. 25 European languages. Models: {size}."),
            Engine::Parakeet,
        )
    };
    let size = engine.models().iter().map(|model| model.size()).sum();
    model_row.set_subtitle(&description.replace("{size}", &glib::format_size(size)));
    language_row.set_sensitive(whisper);
    language_row.set_subtitle(&if whisper {
        gettext("Choosing the language avoids misrecognised short phrases")
    } else {
        gettext("Parakeet detects the language from the speech")
    });
}
