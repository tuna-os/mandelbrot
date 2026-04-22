use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio, glib,
    glib::{clone, closure_local},
};
use tracing::error;

mod icon_item_row;
mod room_row;
mod row;
mod section_row;
mod verification_row;

use self::{
    icon_item_row::SidebarIconItemRow, room_row::SidebarRoomRow, row::SidebarRow,
    section_row::SidebarSectionRow, verification_row::SidebarVerificationRow,
};
use crate::{
    account_settings::{AccountSettings, AccountSettingsSubpage},
    account_switcher::AccountSwitcherButton,
    components::OfflineBanner,
    session::{
        CryptoIdentityState, RecoveryState, RoomCategory, Session, SessionVerificationState,
        SidebarListModel, SidebarSection, TargetRoomCategory, User,
    },
    utils::FixedSelection,
};

mod imp {
    use std::{
        cell::{Cell, OnceCell, RefCell},
        sync::LazyLock,
    };

    use glib::subclass::{InitializingObject, Signal};

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate, glib::Properties)]
    #[template(resource = "/org/gnome/Fractal/ui/session_view/sidebar/mod.ui")]
    #[properties(wrapper_type = super::Sidebar)]
    pub struct Sidebar {
        #[template_child]
        pub(super) header_bar: TemplateChild<adw::HeaderBar>,
        #[template_child]
        account_switcher_button: TemplateChild<AccountSwitcherButton>,
        #[template_child]
        security_banner: TemplateChild<adw::Banner>,
        #[template_child]
        scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        listview: TemplateChild<gtk::ListView>,
        #[template_child]
        room_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        room_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) room_search: TemplateChild<gtk::SearchBar>,
        #[template_child]
        room_row_menu: TemplateChild<gio::MenuModel>,
        room_row_popover: OnceCell<gtk::PopoverMenu>,
        /// The logged-in user.
        #[property(get, set = Self::set_user, explicit_notify, nullable)]
        user: RefCell<Option<User>>,
        /// The category of the source that activated drop mode.
        pub(super) drop_source_category: Cell<Option<RoomCategory>>,
        /// The category of the drop target that is currently hovered.
        pub(super) drop_active_target_category: Cell<Option<TargetRoomCategory>>,
        /// The list model of this sidebar.
        #[property(get, set = Self::set_list_model, explicit_notify, nullable)]
        list_model: glib::WeakRef<SidebarListModel>,
        session_handler: RefCell<Option<glib::SignalHandlerId>>,
        security_handlers: RefCell<Vec<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sidebar {
        const NAME: &'static str = "Sidebar";
        type Type = super::Sidebar;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            OfflineBanner::ensure_type();

            Self::bind_template(klass);
            Self::bind_template_callbacks(klass);

            klass.set_css_name("sidebar");
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Sidebar {
        fn signals() -> &'static [Signal] {
            static SIGNALS: LazyLock<Vec<Signal>> = LazyLock::new(|| {
                vec![
                    Signal::builder("drop-source-category-changed").build(),
                    Signal::builder("drop-active-target-category-changed").build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(clone!(
                #[weak]
                obj,
                move |_, item| {
                    let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                        error!("List item factory did not receive a list item: {item:?}");
                        return;
                    };
                    let row = SidebarRow::new(&obj);
                    item.set_child(Some(&row));
                    item.bind_property("item", &row, "item").build();
                }
            ));
            self.listview.set_factory(Some(&factory));

            self.listview.connect_activate(move |listview, pos| {
                let Some(model) = listview.model().and_downcast::<FixedSelection>() else {
                    return;
                };
                let Some(item) = model.item(pos) else {
                    return;
                };

                if let Some(section) = item.downcast_ref::<SidebarSection>() {
                    section.set_is_expanded(!section.is_expanded());
                } else {
                    model.set_selected(pos);
                }
            });

            obj.property_expression("list-model")
                .chain_property::<SidebarListModel>("selection-model")
                .bind(&*self.listview, "model", None::<&glib::Object>);

            // FIXME: Remove this hack once https://gitlab.gnome.org/GNOME/gtk/-/issues/4938 is resolved
            self.scrolled_window
                .vscrollbar()
                .first_child()
                .unwrap()
                .set_overflow(gtk::Overflow::Hidden);

            // Use the built-in search-changed signal which is already
            // debounced by the search-delay property (default 150ms).
            self.room_search_entry.connect_search_changed(clone!(
                #[weak(rename_to = imp)]
                self,
                move |_| {
                    imp.update_search_filter();
                }
            ));
        }

        fn dispose(&self) {
            if let Some(user) = self.user.take() {
                let session = user.session();
                if let Some(handler) = self.session_handler.take() {
                    session.disconnect(handler);
                }

                let security = session.security();
                for handler in self.security_handlers.take() {
                    security.disconnect(handler);
                }
            }
        }
    }

    impl WidgetImpl for Sidebar {
        fn grab_focus(&self) -> bool {
            if self.listview.grab_focus() {
                true
            } else {
                self.account_switcher_button.grab_focus()
            }
        }
    }

    impl NavigationPageImpl for Sidebar {}

    #[gtk::template_callbacks]
    impl Sidebar {
        /// Set the logged-in user.
        fn set_user(&self, user: Option<User>) {
            let prev_user = self.user.borrow().clone();
            if prev_user == user {
                return;
            }

            if let Some(user) = prev_user {
                let session = user.session();
                if let Some(handler) = self.session_handler.take() {
                    session.disconnect(handler);
                }

                let security = session.security();
                for handler in self.security_handlers.take() {
                    security.disconnect(handler);
                }
            }

            if let Some(user) = &user {
                let session = user.session();

                let offline_handler = session.connect_is_offline_notify(clone!(
                    #[weak(rename_to = imp)]
                    self,
                    move |_| {
                        imp.update_security_banner();
                    }
                ));
                self.session_handler.replace(Some(offline_handler));

                let security = session.security();
                let crypto_identity_handler =
                    security.connect_crypto_identity_state_notify(clone!(
                        #[weak(rename_to = imp)]
                        self,
                        move |_| {
                            imp.update_security_banner();
                        }
                    ));
                let verification_handler = security.connect_verification_state_notify(clone!(
                    #[weak(rename_to = imp)]
                    self,
                    move |_| {
                        imp.update_security_banner();
                    }
                ));
                let recovery_handler = security.connect_recovery_state_notify(clone!(
                    #[weak(rename_to = imp)]
                    self,
                    move |_| {
                        imp.update_security_banner();
                    }
                ));

                self.security_handlers.replace(vec![
                    crypto_identity_handler,
                    verification_handler,
                    recovery_handler,
                ]);
            }

            self.user.replace(user);

            self.update_security_banner();
            self.obj().notify_user();
        }

        /// Set the list model of the sidebar.
        fn set_list_model(&self, list_model: Option<&SidebarListModel>) {
            if self.list_model.upgrade().as_ref() == list_model {
                return;
            }
            let obj = self.obj();

            self.list_model.set(list_model);

            if let Some(list_model) = list_model {
                self.update_search_filter();

                self.update_room_stack(&list_model.selection_model());

                list_model.selection_model().connect_is_empty_notify(clone!(
                    #[weak(rename_to = imp)]
                    self,
                    move |list| {
                        imp.update_room_stack(list);
                    }
                ));
            }

            obj.notify_list_model();
        }

        /// Update the search filter with the current search text.
        ///
        /// Called when the search term changes and when the list model is set.
        fn update_search_filter(&self) {
            let Some(list_model) = self.list_model.upgrade() else {
                return;
            };

            let text = self.room_search_entry.text();
            let normalized = secular::normalized_lower_lay_string(&text);
            list_model.string_filter().set_search(Some(&normalized));
        }

        /// The current session, if any.
        fn session(&self) -> Option<Session> {
            self.user.borrow().as_ref().map(User::session)
        }

        /// Switch between room list and placeholder status page
        /// depending on if the users search has any results or not
        fn update_room_stack(&self, list: &FixedSelection) {
            self.room_stack.set_visible_child_name(if list.is_empty() {
                "no-results"
            } else {
                "room-list"
            });
        }

        /// Update the security banner.
        fn update_security_banner(&self) {
            let Some(session) = self.session() else {
                return;
            };

            if session.is_offline() {
                // Only show one banner at a time.
                // The user will not be able to solve security issues while offline anyway.
                self.security_banner.set_revealed(false);
                return;
            }

            let security = session.security();
            let crypto_identity_state = security.crypto_identity_state();
            let verification_state = security.verification_state();
            let recovery_state = security.recovery_state();

            if crypto_identity_state == CryptoIdentityState::Unknown
                || verification_state == SessionVerificationState::Unknown
                || recovery_state == RecoveryState::Unknown
            {
                // Do not show the banner prematurely, unknown states should solve themselves.
                self.security_banner.set_revealed(false);
                return;
            }

            if verification_state == SessionVerificationState::Verified
                && recovery_state == RecoveryState::Enabled
            {
                // No need for the banner.
                self.security_banner.set_revealed(false);
                return;
            }

            let (title, button) = if crypto_identity_state == CryptoIdentityState::Missing {
                (gettext("No crypto identity"), gettext("Enable"))
            } else if verification_state == SessionVerificationState::Unverified {
                (gettext("Crypto identity incomplete"), gettext("Verify"))
            } else {
                match recovery_state {
                    RecoveryState::Disabled => {
                        (gettext("Account recovery disabled"), gettext("Enable"))
                    }
                    RecoveryState::Incomplete => {
                        (gettext("Account recovery incomplete"), gettext("Recover"))
                    }
                    _ => unreachable!(),
                }
            };

            self.security_banner.set_title(&title);
            self.security_banner.set_button_label(Some(&button));
            self.security_banner.set_revealed(true);
        }

        /// Set the category of the source that activated drop mode.
        pub(super) fn set_drop_source_category(&self, source_category: Option<RoomCategory>) {
            if self.drop_source_category.get() == source_category {
                return;
            }

            self.drop_source_category.set(source_category);

            if source_category.is_some() {
                self.listview.add_css_class("drop-mode");
            } else {
                self.listview.remove_css_class("drop-mode");
            }

            let Some(item_list) = self.list_model.upgrade().map(|model| model.item_list()) else {
                return;
            };

            item_list.set_show_all_for_room_category(source_category);
            self.obj()
                .emit_by_name::<()>("drop-source-category-changed", &[]);
        }

        /// The shared popover for a room row in the sidebar.
        pub(super) fn room_row_popover(&self) -> &gtk::PopoverMenu {
            self.room_row_popover.get_or_init(|| {
                let popover = gtk::PopoverMenu::builder()
                    .menu_model(&*self.room_row_menu)
                    .has_arrow(false)
                    .halign(gtk::Align::Start)
                    .build();
                popover
                    .update_property(&[gtk::accessible::Property::Label(&gettext("Context Menu"))]);

                popover
            })
        }

        /// Scroll to the currently selected item of the sidebar.
        pub(super) fn scroll_to_selection(&self) {
            let Some(list_model) = self.list_model.upgrade() else {
                return;
            };

            let selected = list_model.selection_model().selected();

            if selected != gtk::INVALID_LIST_POSITION {
                self.listview
                    .scroll_to(selected, gtk::ListScrollFlags::FOCUS, None);
            }
        }

        /// Open the proper security flow to fix the current issue.
        #[template_callback]
        fn fix_security_issue(&self) {
            let Some(session) = self.session() else {
                return;
            };

            let dialog = AccountSettings::new(&session);

            // Show the encryption tab if the user uses the back button.
            dialog.show_encryption_tab();

            let security = session.security();
            let crypto_identity_state = security.crypto_identity_state();
            let verification_state = security.verification_state();

            let subpage = if crypto_identity_state == CryptoIdentityState::Missing
                || verification_state == SessionVerificationState::Unverified
            {
                AccountSettingsSubpage::CryptoIdentitySetup
            } else {
                AccountSettingsSubpage::RecoverySetup
            };
            dialog.show_subpage(subpage);

            dialog.present(Some(&*self.obj()));
        }

        /// Select the first room found in search result,
        /// or noop if no search term has been entered.
        #[template_callback]
        fn activate_first_search_result(&self) {
            if self.room_search_entry.text().is_empty() {
                return;
            }

            let _ = self
                .listview
                .activate_action("list.activate-item", Some(&0u32.into()));
        }
    }
}

glib::wrapper! {
    /// The sidebar of the session view, displaying the list of rooms
    /// available for the current session, among other things.
    pub struct Sidebar(ObjectSubclass<imp::Sidebar>)
        @extends gtk::Widget, adw::NavigationPage,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl Sidebar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The search bar allowing to filter rooms in the sidebar.
    pub(crate) fn room_search_bar(&self) -> gtk::SearchBar {
        self.imp().room_search.clone()
    }

    /// The category of the source that activated drop mode.
    fn drop_source_category(&self) -> Option<RoomCategory> {
        self.imp().drop_source_category.get()
    }

    /// Set the category of the source that activated drop mode.
    fn set_drop_source_category(&self, source_category: Option<RoomCategory>) {
        self.imp().set_drop_source_category(source_category);
    }

    /// The category of the drop target that is currently hovered.
    fn drop_active_target_category(&self) -> Option<TargetRoomCategory> {
        self.imp().drop_active_target_category.get()
    }

    /// Set the category of the drop target that is currently hovered.
    fn set_drop_active_target_category(&self, target_category: Option<TargetRoomCategory>) {
        if self.drop_active_target_category() == target_category {
            return;
        }

        self.imp().drop_active_target_category.set(target_category);
        self.emit_by_name::<()>("drop-active-target-category-changed", &[]);
    }

    /// The shared popover for a room row in the sidebar.
    fn room_row_popover(&self) -> &gtk::PopoverMenu {
        self.imp().room_row_popover()
    }

    /// The `AdwHeaderBar` of the sidebar.
    pub(crate) fn header_bar(&self) -> &adw::HeaderBar {
        &self.imp().header_bar
    }

    /// Scroll to the currently selected item of the sidebar.
    pub(crate) fn scroll_to_selection(&self) {
        self.imp().scroll_to_selection();
    }

    /// Connect to the signal emitted when the drop source category changed.
    pub fn connect_drop_source_category_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "drop-source-category-changed",
            true,
            closure_local!(move |obj: Self| {
                f(&obj);
            }),
        )
    }

    /// Connect to the signal emitted when the drop active target category
    /// changed.
    pub fn connect_drop_active_target_category_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "drop-active-target-category-changed",
            true,
            closure_local!(move |obj: Self| {
                f(&obj);
            }),
        )
    }
}
