//! Plays the audio of the open file for the media controls.
//!
//! `gtk::MediaFile` is not used: it plays through `playbin3`, whose
//! `decodebin3` in GStreamer 1.28 aborts the whole process on loading an MP3
//! file, with `mq_slot_handle_stream_start: assertion failed: (collection)`.
//! `playbin`, built on `decodebin`, does not have that bug.

use std::cell::{OnceCell, RefCell};
use std::time::Duration;

use gst::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, prelude::*};

/// How often the playback position is reported while playing.
const TICK: Duration = Duration::from_millis(100);

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Player {
        pub playbin: OnceCell<gst::Element>,
        pub bus_watch: RefCell<Option<gst::bus::BusWatchGuard>>,
        pub tick: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Player {
        const NAME: &'static str = "VerbatimPlayer";
        type Type = super::Player;
        type ParentType = gtk::MediaStream;
    }

    impl ObjectImpl for Player {
        fn dispose(&self) {
            self.stop_tick();
            self.bus_watch.take();
            if let Some(playbin) = self.playbin.get() {
                let _ = playbin.set_state(gst::State::Null);
            }
        }
    }

    impl MediaStreamImpl for Player {
        fn play(&self) -> bool {
            let playbin = self.playbin();
            if self.obj().is_ended() {
                let _ = playbin.seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO);
            }
            if playbin.set_state(gst::State::Playing).is_err() {
                return false;
            }
            self.start_tick();
            true
        }

        fn pause(&self) {
            self.stop_tick();
            let _ = self.playbin().set_state(gst::State::Paused);
        }

        fn seek(&self, timestamp: i64) {
            let position = gst::ClockTime::from_useconds(timestamp.max(0) as u64);
            let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE;
            if self.playbin().seek_simple(flags, position).is_err() {
                self.obj().seek_failed();
            }
        }

        fn update_audio(&self, muted: bool, volume: f64) {
            let playbin = self.playbin();
            playbin.set_property("mute", muted);
            playbin.set_property("volume", volume);
        }
    }

    impl Player {
        pub fn playbin(&self) -> &gst::Element {
            self.playbin.get().expect("set when the player is created")
        }

        fn start_tick(&self) {
            self.stop_tick();
            let player = self.obj().downgrade();
            let id = glib::timeout_add_local(TICK, move || {
                let Some(player) = player.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                player.imp().report_position();
                glib::ControlFlow::Continue
            });
            self.tick.replace(Some(id));
        }

        pub fn stop_tick(&self) {
            if let Some(id) = self.tick.take() {
                id.remove();
            }
        }

        pub fn report_position(&self) {
            if let Some(position) = self.playbin().query_position::<gst::ClockTime>() {
                self.obj().update(position.useconds() as i64);
            }
        }

        pub fn handle_message(&self, msg: &gst::Message) {
            let obj = self.obj();
            match msg.view() {
                // Emitted once the file has prerolled, and after each
                // flushing seek.
                gst::MessageView::AsyncDone(_) => {
                    if !obj.is_prepared() {
                        let playbin = self.playbin();
                        let duration = playbin
                            .query_duration::<gst::ClockTime>()
                            .map_or(0, |duration| duration.useconds() as i64);
                        let mut query = gst::query::Seeking::new(gst::Format::Time);
                        let seekable = playbin.query(&mut query) && query.result().0;
                        obj.stream_prepared(true, false, seekable, duration);
                    } else if obj.is_seeking() {
                        obj.seek_success();
                        self.report_position();
                    }
                }
                gst::MessageView::Eos(_) => {
                    self.stop_tick();
                    let _ = self.playbin().set_state(gst::State::Paused);
                    obj.stream_ended();
                }
                gst::MessageView::Error(err) => {
                    self.stop_tick();
                    let _ = self.playbin().set_state(gst::State::Null);
                    obj.set_error(err.error());
                }
                _ => {}
            }
        }
    }
}

glib::wrapper! {
    pub struct Player(ObjectSubclass<imp::Player>)
        @extends gtk::MediaStream,
        @implements gdk::Paintable;
}

impl Player {
    /// Loads `file`, paused at its start.
    pub fn for_file(file: &gio::File) -> Result<Self, glib::Error> {
        gst::init()?;
        let playbin = gst::ElementFactory::make("playbin")
            .property("uri", file.uri())
            .build()
            .map_err(|err| glib::Error::new(gst::CoreError::MissingPlugin, &err.message))?;
        // The transcript has no video view, so video streams and cover art
        // are not decoded.
        playbin.set_property_from_str("flags", "audio");

        let player: Self = glib::Object::new();
        let imp = player.imp();
        let watch = playbin
            .bus()
            .expect("a pipeline always has a bus")
            .add_watch_local(glib::clone!(
                #[weak]
                player,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move |_, msg| {
                    player.imp().handle_message(msg);
                    glib::ControlFlow::Continue
                }
            ))
            .map_err(|err| glib::Error::new(gst::CoreError::Failed, &err.message))?;
        imp.bus_watch.replace(Some(watch));
        imp.playbin
            .set(playbin)
            .expect("the player is created once");
        if let Err(err) = imp.playbin().set_state(gst::State::Paused) {
            return Err(glib::Error::new(
                gst::CoreError::StateChange,
                &err.to_string(),
            ));
        }
        Ok(player)
    }
}
