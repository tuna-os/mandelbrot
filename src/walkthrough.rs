//! Screenshot walkthrough mode.
//!
//! **Inactive unless `MANDELBROT_WALKTHROUGH=1` is set**: [`is_enabled`] is the
//! only entry point the rest of the app calls, and it returns `false` in every
//! normal run, so nothing below ever executes for a user.
//!
//! The app holds an ordered list of named steps. A GLib timer runs each step,
//! polls its readiness predicate until the UI has settled, then prints
//! `WALKTHROUGH-SHOT <name>` to stdout and flushes. `scripts/walkthrough.sh`
//! reads those markers and grabs one frame per marker into `docs/guide/`.
//! After the last step the app quits, so CI can gate on the final marker
//! (`about`) ever appearing: reaching it proves every UI state below was
//! constructed and rendered without crashing.
//!
//! Two passes, selected by whether credentials are in the environment:
//!
//! * **login pass** (no credentials): the states reachable without a Matrix
//!   session — greeter, QR login, homeserver, method, call UI via its demo
//!   mode, the poll dialog, the error page, About.
//! * **session pass** (`MANDELBROT_WALKTHROUGH_USER`, `_PASSWORD` and
//!   `_HOMESERVER` set): logs in against the local e2e homeserver seeded by
//!   `scripts/walkthrough-seed.sh` and walks the feature UI — room list, a
//!   populated timeline, the thread panel, the space overview, account
//!   settings.

use std::{cell::Cell, env, rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::glib;
use tracing::{info, warn};

use crate::{
    Window,
    session::Session,
    session_view::{CallDialog, CallState, CreatePollDialog},
    spawn, spawn_tokio,
};

/// The environment variable enabling walkthrough mode.
const ENABLE_VAR: &str = "MANDELBROT_WALKTHROUGH";

/// How often the readiness predicate of the current step is polled.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// How long a step may wait for its readiness predicate before giving up and
/// being captured anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(25);
/// How long to let the frame settle (animations, image loading) after the step
/// is ready, before printing its marker.
const SETTLE: Duration = Duration::from_millis(1_800);

/// Whether walkthrough mode is enabled.
pub(crate) fn is_enabled() -> bool {
    env::var(ENABLE_VAR).is_ok_and(|value| !value.is_empty() && value != "0")
}

/// Read a walkthrough environment variable, without the common prefix.
fn var(suffix: &str) -> Option<String> {
    env::var(format!("{ENABLE_VAR}_{suffix}"))
        .ok()
        .filter(|value| !value.is_empty())
}

/// A single walkthrough step.
struct Step {
    /// The name of the step, used as the marker and the screenshot file name.
    name: &'static str,
    /// The action putting the UI into the state to capture.
    action: Box<dyn Fn()>,
    /// Whether the state is ready to be captured.
    ///
    /// Polled every [`POLL_INTERVAL`] until it returns `true` or
    /// [`READY_TIMEOUT`] elapses. Gating on observed state rather than a fixed
    /// sleep is what keeps the seeded screenshots deterministic: a login and a
    /// sliding-sync round trip take an unpredictable amount of time.
    ready: Box<dyn Fn() -> bool>,
}

impl Step {
    /// A step that is ready as soon as its action has run.
    fn now(name: &'static str, action: impl Fn() + 'static) -> Self {
        Self {
            name,
            action: Box::new(action),
            ready: Box::new(|| true),
        }
    }

    /// A step that waits for `ready` before being captured.
    fn when(
        name: &'static str,
        action: impl Fn() + 'static,
        ready: impl Fn() -> bool + 'static,
    ) -> Self {
        Self {
            name,
            action: Box::new(action),
            ready: Box::new(ready),
        }
    }
}

/// Start walkthrough mode on the given window.
///
/// Callers must check [`is_enabled`] first.
pub(crate) fn start(window: &Window) {
    // No window manager under Xvfb, so `maximize()` is ignored: size the
    // window explicitly so the guide screenshots have a consistent 16:10 shape
    // and no dead space.
    window.set_default_size(1400, 950);

    let Some(user) = var("USER") else {
        run(window, login_pass(window));
        return;
    };

    let homeserver = var("HOMESERVER").unwrap_or_else(|| "http://localhost:8008".to_owned());
    let password = var("PASSWORD").unwrap_or_default();

    let window = window.clone();
    spawn!(async move {
        match log_in(&homeserver, &user, &password).await {
            Ok(session) => {
                session.prepare().await;
                window.add_session(session);
                let steps = session_pass(&window);
                run(&window, steps);
            }
            Err(error) => {
                warn!("Walkthrough could not log in: {error}");
                // Fall back to the login pass rather than hanging: the driver
                // then fails validation on the missing session screenshots.
                let steps = login_pass(&window);
                run(&window, steps);
            }
        }
    });
}

/// Log in with a password and build a session from the result.
async fn log_in(homeserver: &str, user: &str, password: &str) -> Result<Session, String> {
    let homeserver = homeserver.to_owned();
    let user = user.to_owned();
    let password = password.to_owned();

    let handle = spawn_tokio!(async move {
        let client = matrix_sdk::Client::builder()
            .respect_login_well_known(false)
            .homeserver_url(&homeserver)
            .build()
            .await
            .map_err(|error| error.to_string())?;

        client
            .matrix_auth()
            .login_username(&user, &password)
            .initial_device_display_name("Mandelbrot Walkthrough")
            .await
            .map_err(|error| error.to_string())?;

        Ok::<_, String>(client)
    });

    let client = handle.await.expect("task was not aborted")?;

    Session::create(&client)
        .await
        .map_err(|error| error.to_string())
}

/// The steps of the login pass, walking the states reachable without a Matrix
/// session.
fn login_pass(window: &Window) -> Vec<Step> {
    let mut steps = Vec::new();

    // The greeter is the initial page of the login view, including the "Sign
    // in with QR Code" entry.
    {
        let window = window.clone();
        steps.push(Step::now("login-greeter", move || {
            gtk::prelude::WidgetExt::activate_action(&window, "win.new-session", None).ok();
        }));
    }

    // The pages of the login navigation stack are reachable by their tag
    // through the same `navigation.push` action the greeter buttons use.
    for (name, tag) in [
        ("login-qr-code", "qr-code"),
        ("login-homeserver", "homeserver"),
        ("login-method", "method"),
    ] {
        let window = window.clone();
        steps.push(Step::now(name, move || {
            push_login_page(&window, tag);
        }));
    }

    {
        let window = window.clone();
        steps.push(Step::now("login-greeter-back", move || {
            pop_login_pages(&window);
        }));
    }

    steps.extend(shared_steps(window));
    steps
}

/// The steps of the session pass, walking the feature UI of a logged-in,
/// seeded session.
fn session_pass(window: &Window) -> Vec<Step> {
    let mut steps = Vec::new();
    let session_id = window.current_session_id();

    // The room list only fills in once the first sliding sync has completed.
    {
        let window = window.clone();
        let expected = var("ROOMS")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1);
        let ready_window = window.clone();
        steps.push(Step::when(
            "session-room-list",
            move || {
                gtk::prelude::WidgetExt::activate_action(&window, "win.show-session", None).ok();
            },
            move || room_count(&ready_window) >= expected,
        ));
    }

    // The seeding script reports the IDs of the fixtures it created, so the
    // app never has to guess which room is which.
    if let Some(room_id) = var("ROOM") {
        let target = window.clone();
        let ready_window = window.clone();
        let ready_room = room_id.clone();
        // Opening a room starts a timeline load of its own, separate from the
        // sliding sync the room list waited on. Hold the shot until the room
        // has been selected and then stayed selected for a few seconds, so the
        // events, avatars and the poll have had time to land.
        let settled = Cell::new(0u32);
        steps.push(Step::when(
            "room-timeline",
            move || show_room(&target, &room_id),
            move || {
                let open = ready_window
                    .session_view()
                    .selected_room()
                    .is_some_and(|room| room.room_id().as_str() == ready_room);
                settled.set(if open { settled.get() + 1 } else { 0 });
                settled.get() >= 24
            },
        ));

        if let Some(thread_root) = var("THREAD_ROOT") {
            let window = window.clone();
            steps.push(Step::now("room-thread-panel", move || {
                gtk::prelude::WidgetExt::activate_action(
                    window.session_view(),
                    "room-history.show-thread",
                    Some(&thread_root.to_variant()),
                )
                .ok();
            }));
        }

        {
            let window = window.clone();
            steps.push(Step::now("room-timeline-again", move || {
                gtk::prelude::WidgetExt::activate_action(
                    window.session_view(),
                    "room-history.close-thread",
                    None,
                )
                .ok();
            }));
        }
    }

    if let Some(space_id) = var("SPACE") {
        let window = window.clone();
        steps.push(Step::now("space-overview", move || {
            show_room(&window, &space_id);
        }));
    }

    if let Some(session_id) = session_id {
        let window = window.clone();
        steps.push(Step::now("account-settings", move || {
            gtk::prelude::WidgetExt::activate_action(
                &window,
                "win.open-account-settings",
                Some(&session_id.to_variant()),
            )
            .ok();
        }));
    }

    steps.extend(shared_steps(window));
    steps
}

/// The steps shared by both passes: standalone dialogs and the error page.
///
/// The last one must stay named `about`: CI gates on that marker.
fn shared_steps(window: &Window) -> Vec<Step> {
    let mut steps = Vec::new();

    // The call UI has no live SFU in a walkthrough, so drive it through the
    // demo mode that already exists for exactly this purpose.
    let call_state = CallState::new();
    let call_dialog = CallDialog::new(&call_state);

    {
        let window = window.clone();
        let dialog = call_dialog.clone();
        steps.push(Step::now("call-prescreen", move || {
            dialog.present(Some(&window));
        }));
    }
    {
        let state = call_state.clone();
        steps.push(Step::when("call-view", move || state.start_demo(), {
            let state = call_state.clone();
            move || state.participant_count() >= 3
        }));
    }
    {
        let dialog = call_dialog.clone();
        steps.push(Step::now("call-closed", move || {
            dialog.close();
        }));
    }

    {
        let window = window.clone();
        steps.push(Step::now("create-poll", move || {
            CreatePollDialog::new().present(Some(&window));
        }));
    }
    {
        let window = window.clone();
        steps.push(Step::now("create-poll-closed", move || {
            close_top_dialog(&window);
        }));
    }

    {
        let window = window.clone();
        steps.push(Step::now("error-page", move || {
            window.show_secret_error("The session data could not be read from the secret store.");
        }));
    }

    {
        let window = window.clone();
        steps.push(Step::now("about", move || {
            gtk::prelude::WidgetExt::activate_action(&window, "app.about", None).ok();
        }));
    }

    steps
}

/// Push the login page with the given tag.
fn push_login_page(window: &Window, tag: &str) {
    gtk::prelude::WidgetExt::activate_action(window, "win.new-session", None).ok();
    if let Some(view) = find_navigation_view(window.upcast_ref::<gtk::Widget>()) {
        view.push_by_tag(tag);
    }
}

/// Pop the login navigation stack back to the greeter.
fn pop_login_pages(window: &Window) {
    if let Some(view) = find_navigation_view(window.upcast_ref::<gtk::Widget>()) {
        view.pop_to_tag("greeter");
    }
}

/// Find the login navigation view, i.e. the first one holding a `greeter` page.
fn find_navigation_view(widget: &gtk::Widget) -> Option<adw::NavigationView> {
    if let Some(view) = widget.downcast_ref::<adw::NavigationView>()
        && view.find_page("greeter").is_some()
    {
        return Some(view.clone());
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(view) = find_navigation_view(&current) {
            return Some(view);
        }
        child = current.next_sibling();
    }

    None
}

/// Show the room with the given ID in the session view.
fn show_room(window: &Window, room_id: &str) {
    gtk::prelude::WidgetExt::activate_action(
        window.session_view(),
        "session.show-room",
        Some(&room_id.to_variant()),
    )
    .ok();
}

/// The number of rooms the current session knows about.
fn room_count(window: &Window) -> usize {
    window
        .session_view()
        .session()
        .map_or(0, |session| session.room_list().snapshot().len())
}

/// Close the topmost dialog of the window, if any.
fn close_top_dialog(window: &Window) {
    if let Some(dialog) = window.visible_dialog() {
        dialog.close();
    }
}

/// Run the given steps, printing a marker after each has settled, then quit.
fn run(window: &Window, steps: Vec<Step>) {
    /// What the driver loop is doing with the current step.
    enum Phase {
        /// The action has not run yet.
        Act,
        /// Waiting for the readiness predicate.
        Wait,
        /// Ready; letting the frame settle before printing the marker.
        Settle,
    }

    let steps = Rc::new(steps);
    let index = Cell::new(0usize);
    let phase = std::cell::RefCell::new(Phase::Act);
    let waited = Cell::new(Duration::ZERO);
    let window = window.clone();

    info!("Walkthrough mode started with {} steps", steps.len());

    glib::timeout_add_local(POLL_INTERVAL, move || {
        let Some(step) = steps.get(index.get()) else {
            // Give the driver time to grab the last frame, then exit.
            let window = window.clone();
            glib::timeout_add_local_once(SETTLE, move || {
                if let Some(app) = window.application() {
                    app.quit();
                } else {
                    window.close();
                }
            });
            return glib::ControlFlow::Break;
        };

        let next = match *phase.borrow() {
            Phase::Act => {
                (step.action)();
                waited.set(Duration::ZERO);
                Phase::Wait
            }
            Phase::Wait => {
                let elapsed = waited.get() + POLL_INTERVAL;
                waited.set(elapsed);

                if (step.ready)() {
                    waited.set(Duration::ZERO);
                    Phase::Settle
                } else if elapsed >= READY_TIMEOUT {
                    warn!(
                        "Walkthrough step `{}` timed out waiting to settle",
                        step.name
                    );
                    waited.set(Duration::ZERO);
                    Phase::Settle
                } else {
                    Phase::Wait
                }
            }
            Phase::Settle => {
                let elapsed = waited.get() + POLL_INTERVAL;
                waited.set(elapsed);

                if elapsed < SETTLE {
                    Phase::Settle
                } else {
                    use std::io::Write as _;
                    println!("WALKTHROUGH-SHOT {}", step.name);
                    let _ = std::io::stdout().flush();
                    index.set(index.get() + 1);
                    Phase::Act
                }
            }
        };
        *phase.borrow_mut() = next;

        glib::ControlFlow::Continue
    });
}
