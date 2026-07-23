// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure-logic MatrixRTC session and membership management for Mandelbrot.
//!
//! This crate reimplements the session/membership logic of `matrix-js-sdk`'s
//! `matrixrtc` module in Rust. It does not perform any I/O itself: the
//! application provides an implementation of [`RtcClientApi`] on top of its
//! Matrix stack, and feeds room state into [`MatrixRtcSession`].
//!
//! The current scope is membership management only (no encryption manager,
//! no LiveKit integration):
//!
//! - [`CallMembership`] parses and validates `m.call.member` state events in
//!   the [`SessionMembershipData`] format.
//! - [`MembershipManager`] is the join/leave state machine with MSC4140
//!   delayed leave events.
//! - [`MatrixRtcSession`] builds the membership list of the room call from
//!   room state.

pub mod call_membership;
pub mod client;
pub mod membership_data;
pub mod membership_manager;
pub mod session;

pub use call_membership::{CallMembership, MemberStateEvent};
pub use client::{
    ClientError, RtcClientApi, SendDelayedEventResponse, SendEventResponse,
    UpdateDelayedEventAction,
};
pub use membership_data::{
    DEFAULT_EXPIRE_DURATION_MS, FocusActive, MembershipParseError, SessionMembershipData,
    Transport, parse_session_membership_data,
};
pub use membership_manager::{
    MembershipConfig, MembershipManager, MembershipManagerEvent, RtcRoom, Status,
};
pub use session::{MatrixRtcSession, SlotDescription};
