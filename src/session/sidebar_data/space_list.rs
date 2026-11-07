use std::{collections::HashMap, time::Duration};

use futures_util::StreamExt;
use gtk::{gio, glib, glib::clone, prelude::*, subclass::prelude::*};
use matrix_sdk::{Client, deserialized_responses::RawSyncOrStrippedState};
use ruma::{
    OwnedRoomId, RoomId,
    events::{SyncStateEvent, space::child::SpaceChildEventContent},
};
use serde::Deserialize;
use tokio::task::AbortHandle;
use tokio_stream::wrappers::BroadcastStream;
use tracing::warn;

use super::{
    SidebarSpaceItem,
    space_graph::{JoinedSpace, SpaceGraph, SpaceVisibility},
};
use crate::{
    session::{Room, RoomList, SessionSettings},
    spawn, spawn_tokio,
};

/// The delay to wait for more changes before rebuilding the graph of spaces.
const REBUILD_DELAY: Duration = Duration::from_millis(250);

mod imp {
    use std::{
        cell::{Cell, OnceCell, RefCell},
        marker::PhantomData,
    };

    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::SidebarSpaceList)]
    pub struct SidebarSpaceList {
        /// The list of rooms of the session.
        #[property(get, set = Self::set_room_list, construct_only)]
        room_list: OnceCell<RoomList>,
        /// The "Home" item.
        home: OnceCell<SidebarSpaceItem>,
        /// The items of the top-level spaces.
        top_level: RefCell<Vec<SidebarSpaceItem>>,
        /// The graph of the joined spaces.
        graph: RefCell<SpaceGraph>,
        /// The navigation path requested by the user, from a top-level space
        /// to the selected space.
        ///
        /// It is kept even if it is not valid in the current graph, because
        /// spaces might still be loading.
        requested_path: RefCell<Vec<OwnedRoomId>>,
        /// The navigation path that is applied, the longest valid prefix of
        /// the requested path.
        path: RefCell<Vec<OwnedRoomId>>,
        /// The selected space, or `None` if "Home" is selected.
        #[property(get)]
        current_space: RefCell<Option<Room>>,
        /// Whether the selected space is a subspace, so it is possible to go
        /// back to its parent.
        #[property(get)]
        can_go_back: Cell<bool>,
        /// Whether the user has joined any space.
        #[property(get)]
        has_spaces: Cell<bool>,
        /// The position of the selected item in this list.
        #[property(get = Self::selected_position)]
        selected_position: PhantomData<u32>,
        /// Whether the room search is active.
        ///
        /// When it is active, the rooms of all spaces are visible.
        search_active: Cell<bool>,
        /// The filter for rooms that are not spaces.
        pub(super) room_filter: gtk::CustomFilter,
        /// The filter for spaces.
        pub(super) subspace_filter: gtk::CustomFilter,
        /// Which rooms are visible in the selected space.
        visibility: RefCell<SpaceVisibility>,
        /// The source of the scheduled rebuild of the graph.
        rebuild_source: RefCell<Option<glib::SourceId>>,
        /// Whether the graph is being rebuilt.
        is_rebuilding: Cell<bool>,
        /// Whether the graph needs to be rebuilt again after the ongoing
        /// rebuild.
        needs_rebuild: Cell<bool>,
        /// The abort handle of the task watching room updates.
        updates_abort_handle: RefCell<Option<AbortHandle>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SidebarSpaceList {
        const NAME: &'static str = "SidebarSpaceList";
        type Type = super::SidebarSpaceList;
        type Interfaces = (gio::ListModel,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for SidebarSpaceList {
        fn constructed(&self) {
            self.parent_constructed();

            self.room_filter.set_filter_func(clone!(
                #[weak(rename_to = imp)]
                self,
                #[upgrade_or]
                true,
                move |item| {
                    let Some(room) = item.downcast_ref::<Room>() else {
                        return true;
                    };

                    imp.search_active.get()
                        || imp.visibility.borrow().is_room_visible(room.room_id())
                }
            ));

            self.subspace_filter.set_filter_func(clone!(
                #[weak(rename_to = imp)]
                self,
                #[upgrade_or]
                false,
                move |item| {
                    let Some(room) = item.downcast_ref::<Room>() else {
                        return false;
                    };

                    imp.search_active.get()
                        || imp.visibility.borrow().is_subspace_visible(room.room_id())
                }
            ));

            if let Some(settings) = self.session_settings() {
                self.requested_path.replace(settings.space_path());
            }
        }

        fn dispose(&self) {
            if let Some(source) = self.rebuild_source.take() {
                source.remove();
            }
            if let Some(handle) = self.updates_abort_handle.take() {
                handle.abort();
            }
        }
    }

    impl ListModelImpl for SidebarSpaceList {
        fn item_type(&self) -> glib::Type {
            SidebarSpaceItem::static_type()
        }

        fn n_items(&self) -> u32 {
            1 + self.top_level.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            if position == 0 {
                return Some(self.home().clone().upcast());
            }

            self.top_level
                .borrow()
                .get(position as usize - 1)
                .cloned()
                .and_upcast()
        }
    }

    impl SidebarSpaceList {
        /// The list of rooms of the session.
        fn room_list(&self) -> &RoomList {
            self.room_list
                .get()
                .expect("room list should be initialized")
        }

        /// Set the list of rooms of the session.
        fn set_room_list(&self, room_list: RoomList) {
            let room_list = self.room_list.get_or_init(|| room_list);

            self.home
                .set(SidebarSpaceItem::new(room_list, None))
                .expect("home item should be uninitialized");

            // Rooms that are added or removed might be spaces. This is also the
            // first trigger: the client of the session is ready once there are
            // rooms.
            room_list.connect_items_changed(clone!(
                #[weak(rename_to = imp)]
                self,
                move |_, _, _, _| {
                    imp.schedule_rebuild();
                }
            ));
        }

        /// The "Home" item.
        fn home(&self) -> &SidebarSpaceItem {
            self.home.get().expect("home item should be initialized")
        }

        /// The settings of the current session.
        fn session_settings(&self) -> Option<SessionSettings> {
            Some(self.room_list().session()?.settings())
        }

        /// The position of the selected item in this list.
        fn selected_position(&self) -> u32 {
            let path = self.path.borrow();
            let Some(top_level_id) = path.first() else {
                return 0;
            };

            self.graph
                .borrow()
                .top_level_spaces()
                .iter()
                .position(|id| id == top_level_id)
                .map_or(0, |pos| pos as u32 + 1)
        }

        /// The current navigation path.
        pub(super) fn path(&self) -> Vec<OwnedRoomId> {
            self.path.borrow().clone()
        }

        /// The ID of the top-level space at the given position in this list,
        /// or `None` for "Home".
        pub(super) fn top_level_id_at(&self, position: u32) -> Option<OwnedRoomId> {
            let index = position.checked_sub(1)?;
            self.graph
                .borrow()
                .top_level_spaces()
                .get(index as usize)
                .cloned()
        }

        /// The path of the child space of the selected space with the given
        /// ID, or the shortest path to it from a top-level space.
        pub(super) fn path_to_space(&self, space_id: &RoomId) -> Option<Vec<OwnedRoomId>> {
            let graph = self.graph.borrow();
            let path = self.path.borrow();

            if let Some(current) = path.last()
                && graph.child_spaces(current).contains(space_id)
            {
                let mut new_path = path.clone();
                new_path.push(space_id.to_owned());
                return Some(new_path);
            }

            graph.path_to(space_id)
        }

        /// Whether the room with the given ID is visible in the selected
        /// space.
        pub(super) fn is_room_visible(&self, room_id: &RoomId) -> bool {
            self.visibility.borrow().is_room_visible(room_id)
        }

        /// Whether the space with the given ID is a joined space.
        pub(super) fn is_joined_space(&self, room_id: &RoomId) -> bool {
            self.graph.borrow().is_space(room_id)
        }

        /// Request to navigate to the given path.
        ///
        /// The path is persisted in the settings of the session.
        pub(super) fn request_path(&self, path: Vec<OwnedRoomId>) {
            if let Some(settings) = self.session_settings() {
                settings.set_space_path(path.clone());
            }

            self.requested_path.replace(path);
            self.apply_path(false);
        }

        /// Apply the requested path, according to the current graph.
        fn apply_path(&self, graph_changed: bool) {
            let new_path = self
                .graph
                .borrow()
                .valid_path(&self.requested_path.borrow());

            if !graph_changed && *self.path.borrow() == new_path {
                return;
            }

            let obj = self.obj();
            let _guard = obj.freeze_notify();

            let current_space = new_path.last().and_then(|id| self.room_list().get(id));
            if *self.current_space.borrow() != current_space {
                self.current_space.replace(current_space);
                obj.notify_current_space();
            }

            let can_go_back = new_path.len() > 1;
            if self.can_go_back.get() != can_go_back {
                self.can_go_back.set(can_go_back);
                obj.notify_can_go_back();
            }

            let visibility = SpaceVisibility::new(&self.graph.borrow(), &new_path);
            self.path.replace(new_path);

            let prev_visibility = self.visibility.replace(visibility);
            let (rooms_changed, subspaces_changed) = {
                let visibility = self.visibility.borrow();
                (
                    prev_visibility.rooms != visibility.rooms,
                    prev_visibility.subspaces != visibility.subspaces,
                )
            };

            if rooms_changed {
                self.room_filter.changed(gtk::FilterChange::Different);
            }
            if subspaces_changed {
                self.subspace_filter.changed(gtk::FilterChange::Different);
            }

            obj.notify_selected_position();
        }

        /// Set whether the room search is active.
        pub(super) fn set_search_active(&self, active: bool) {
            if self.search_active.get() == active {
                return;
            }

            self.search_active.set(active);

            let change = if active {
                gtk::FilterChange::LessStrict
            } else {
                gtk::FilterChange::MoreStrict
            };
            self.room_filter.changed(change);
            self.subspace_filter.changed(change);
        }

        /// Schedule a rebuild of the graph of spaces.
        ///
        /// Changes are batched during a short delay.
        fn schedule_rebuild(&self) {
            if self.rebuild_source.borrow().is_some() {
                return;
            }

            let source = glib::timeout_add_local_once(
                REBUILD_DELAY,
                clone!(
                    #[weak(rename_to = imp)]
                    self,
                    move || {
                        imp.rebuild_source.take();
                        imp.rebuild();
                    }
                ),
            );
            self.rebuild_source.replace(Some(source));
        }

        /// Rebuild the graph of spaces from the data in the store.
        fn rebuild(&self) {
            if self.is_rebuilding.get() {
                self.needs_rebuild.set(true);
                return;
            }
            let Some(session) = self.room_list().session() else {
                return;
            };
            let client = session.client();

            self.watch_room_updates(&client);
            self.is_rebuilding.set(true);

            spawn!(clone!(
                #[weak(rename_to = imp)]
                self,
                async move {
                    let handle = spawn_tokio!(async move { load_joined_spaces(&client).await });
                    let spaces = handle.await.expect("task was not aborted");

                    imp.set_graph(SpaceGraph::new(spaces));
                    imp.is_rebuilding.set(false);

                    if imp.needs_rebuild.take() {
                        imp.schedule_rebuild();
                    }
                }
            ));
        }

        /// Watch the room updates of the given client, to rebuild the graph
        /// when spaces change.
        fn watch_room_updates(&self, client: &Client) {
            if self.updates_abort_handle.borrow().is_some() {
                return;
            }

            let stream = BroadcastStream::new(client.subscribe_to_all_room_updates());
            let obj_weak = glib::SendWeakRef::from(self.obj().downgrade());
            let client = client.clone();

            let fut = stream.for_each(move |updates| {
                // If we lagged behind, we might have missed changes to spaces.
                let spaces_changed = updates.as_ref().map_or(true, |updates| {
                    updates
                        .iter_all_room_ids()
                        .any(|room_id| client.get_room(room_id).is_some_and(|room| room.is_space()))
                });
                let obj_weak = obj_weak.clone();

                async move {
                    if !spaces_changed {
                        return;
                    }

                    let ctx = glib::MainContext::default();
                    ctx.spawn(async move {
                        if let Some(obj) = obj_weak.upgrade() {
                            obj.imp().schedule_rebuild();
                        }
                    });
                }
            });

            let handle = spawn_tokio!(fut).abort_handle();
            self.updates_abort_handle.replace(Some(handle));
        }

        /// Set the graph of spaces.
        fn set_graph(&self, graph: SpaceGraph) {
            let room_list = self.room_list();

            let old_items = self
                .top_level
                .borrow()
                .iter()
                .filter_map(|item| Some((item.space()?.room_id().to_owned(), item.clone())))
                .collect::<HashMap<_, _>>();

            let mut new_items = Vec::with_capacity(graph.top_level_spaces().len());
            for space_id in graph.top_level_spaces() {
                let item = old_items.get(space_id).cloned().or_else(|| {
                    room_list
                        .get(space_id)
                        .map(|room| SidebarSpaceItem::new(room_list, Some(&room)))
                });

                if let Some(item) = item {
                    item.set_room_ids(Some(graph.descendants(space_id)));
                    new_items.push(item);
                }
            }

            let has_spaces = !new_items.is_empty();
            self.graph.replace(graph);

            if *self.top_level.borrow() != new_items {
                let removed = self.top_level.borrow().len() as u32;
                let added = new_items.len() as u32;
                self.top_level.replace(new_items);
                self.obj().items_changed(1, removed, added);
            }

            if self.has_spaces.get() != has_spaces {
                self.has_spaces.set(has_spaces);
                self.obj().notify_has_spaces();
            }

            self.apply_path(true);
        }
    }
}

glib::wrapper! {
    /// The list of items of the space switcher of the sidebar: "Home" and the
    /// top-level joined spaces.
    ///
    /// It also holds the navigation state between spaces, and provides the
    /// filters for the rooms of the sidebar according to the selected space.
    pub struct SidebarSpaceList(ObjectSubclass<imp::SidebarSpaceList>)
        @implements gio::ListModel;
}

impl SidebarSpaceList {
    /// Construct a new `SidebarSpaceList` for the given list of rooms.
    pub(crate) fn new(room_list: &RoomList) -> Self {
        glib::Object::builder()
            .property("room-list", room_list)
            .build()
    }

    /// The filter for the rooms of the sidebar that are not spaces.
    ///
    /// It only lets through the rooms that are in the selected space or in
    /// any of its subspaces.
    pub(crate) fn room_filter(&self) -> &gtk::CustomFilter {
        &self.imp().room_filter
    }

    /// The filter for the spaces of the sidebar.
    ///
    /// It only lets through the direct child spaces of the selected space.
    pub(crate) fn subspace_filter(&self) -> &gtk::CustomFilter {
        &self.imp().subspace_filter
    }

    /// Select the item at the given position.
    ///
    /// If `reset` is `false` and the item is the top-level space of the
    /// selected subspace, nothing changes. Otherwise the top-level space is
    /// selected.
    pub(crate) fn select(&self, position: u32, reset: bool) {
        let imp = self.imp();
        let space_id = imp.top_level_id_at(position);

        let path = imp.path();
        if !reset && path.first() == space_id.as_ref() {
            return;
        }

        imp.request_path(space_id.into_iter().collect());
    }

    /// Select the item before or after the selected item, wrapping around.
    pub(crate) fn select_relative(&self, forward: bool) {
        let n_items = self.n_items();
        let current = self.selected_position();

        let position = if forward {
            (current + 1) % n_items
        } else {
            (current + n_items - 1) % n_items
        };

        self.select(position, true);
    }

    /// Navigate to the given space, which should be a joined space.
    ///
    /// If it is a child of the selected space, it is added to the current
    /// path, otherwise the shortest path to it is used.
    pub(crate) fn enter_space(&self, space: &Room) {
        let imp = self.imp();

        if let Some(path) = imp.path_to_space(space.room_id()) {
            imp.request_path(path);
        }
    }

    /// Go back to the parent space of the selected subspace.
    pub(crate) fn go_back(&self) {
        let imp = self.imp();

        let mut path = imp.path();
        if path.len() > 1 {
            path.pop();
            imp.request_path(path);
        }
    }

    /// Make sure that the given room is visible in the sidebar.
    ///
    /// If the room is a joined space, navigate to it. If the room is not in
    /// the selected space, select "Home".
    pub(crate) fn show_room(&self, room: &Room) {
        let imp = self.imp();
        let room_id = room.room_id();

        if imp.is_joined_space(room_id) {
            if imp
                .path()
                .last()
                .is_none_or(|id| id.as_str() != room_id.as_str())
            {
                self.enter_space(room);
            }
        } else if !imp.is_room_visible(room_id) {
            imp.request_path(Vec::new());
        }
    }

    /// Set whether the room search is active.
    ///
    /// When it is active, the rooms of all spaces are visible.
    pub(crate) fn set_search_active(&self, active: bool) {
        self.imp().set_search_active(active);
    }
}

/// Load the data of the joined spaces of the given client from the store.
async fn load_joined_spaces(client: &Client) -> Vec<JoinedSpace> {
    let mut spaces = Vec::new();

    for room in client.joined_space_rooms() {
        let children = match room
            .get_state_events_static::<SpaceChildEventContent>()
            .await
        {
            Ok(events) => events
                .into_iter()
                .filter_map(|event| {
                    let RawSyncOrStrippedState::Sync(raw) = event else {
                        return None;
                    };

                    match raw.deserialize() {
                        // A child without `via` is not a child per the spec.
                        Ok(SyncStateEvent::Original(event)) if !event.content.via.is_empty() => {
                            Some(event.state_key)
                        }
                        _ => None,
                    }
                })
                .collect(),
            Err(error) => {
                warn!(
                    "Could not load the children of space {}: {error}",
                    room.room_id()
                );
                Vec::new()
            }
        };

        spaces.push(JoinedSpace {
            room_id: room.room_id().to_owned(),
            children,
            order: load_space_order(&room).await,
        });
    }

    spaces
}

/// Load the MSC3230 order of the given space from its room account data.
async fn load_space_order(room: &matrix_sdk::Room) -> Option<String> {
    #[derive(Deserialize)]
    struct SpaceOrderEvent {
        content: SpaceOrderEventContent,
    }

    #[derive(Deserialize)]
    struct SpaceOrderEventContent {
        order: String,
    }

    for event_type in ["m.space_order", "org.matrix.msc3230.space_order"] {
        match room.account_data(event_type.into()).await {
            Ok(Some(raw)) => {
                if let Ok(event) = raw.deserialize_as_unchecked::<SpaceOrderEvent>() {
                    return Some(event.content.order);
                }
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    "Could not load the order of space {}: {error}",
                    room.room_id()
                );
            }
        }
    }

    None
}
