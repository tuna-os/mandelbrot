use adw::{prelude::*, subclass::prelude::*};
use geo_uri::GeoUri;
use gettextrs::gettext;
use gtk::glib;
use tracing::warn;

use super::ContentFormat;
use crate::components::LocationViewer;

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(
        resource = "/org/tunaos/mandelbrot/ui/session_view/room_history/message_row/location.ui"
    )]
    pub struct MessageLocation {
        #[template_child]
        overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        location: TemplateChild<LocationViewer>,
        #[template_child]
        overlay_error: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MessageLocation {
        const NAME: &'static str = "ContentMessageLocation";
        type Type = super::MessageLocation;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_css_name("message-location");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MessageLocation {
        fn dispose(&self) {
            self.overlay.unparent();
        }
    }

    impl WidgetImpl for MessageLocation {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if self.location.compact() {
                if orientation == gtk::Orientation::Horizontal {
                    (75, 75, -1, -1)
                } else {
                    (50, 50, -1, -1)
                }
            } else {
                (300, 300, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let width = if self.location.compact() {
                width.min(75)
            } else {
                width
            };
            self.overlay
                .size_allocate(&gtk::Allocation::new(0, 0, width, height), baseline);
        }
    }

    impl MessageLocation {
        /// Set the `geo:` URI to display.
        pub(super) fn set_geo_uri(&self, uri: &str, format: ContentFormat) {
            let compact = matches!(format, ContentFormat::Compact | ContentFormat::Ellipsized);
            self.location.set_compact(compact);

            match GeoUri::parse(uri) {
                Ok(geo_uri) => {
                    self.location.set_location(&geo_uri);
                    self.overlay_error.set_visible(false);
                }
                Err(error) => {
                    warn!("Encountered invalid geo URI: {error}");
                    self.location.set_visible(false);
                    self.overlay_error.set_tooltip_text(Some(&gettext(
                        "Location is invalid and cannot be displayed",
                    )));
                    self.overlay_error.set_visible(true);
                }
            }

            let obj = self.obj();
            if compact {
                obj.set_halign(gtk::Align::Start);
                obj.add_css_class("compact");
            } else {
                obj.set_halign(gtk::Align::Fill);
                obj.remove_css_class("compact");
            }
        }
    }
}

glib::wrapper! {
    /// A widget displaying a location message in the timeline.
    pub struct MessageLocation(ObjectSubclass<imp::MessageLocation>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MessageLocation {
    /// Create a new location message.
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Set the `geo:` URI to display.
    pub(crate) fn set_geo_uri(&self, uri: &str, format: ContentFormat) {
        self.imp().set_geo_uri(uri, format);
    }
}

impl Default for MessageLocation {
    fn default() -> Self {
        Self::new()
    }
}
