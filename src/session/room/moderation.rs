use futures_util::future::join_all;
use matrix_sdk::RoomState;
use ruma::{EventId, OwnedEventId, OwnedUserId, UserId};
use tracing::error;

use super::{Event, Room};
use crate::spawn_tokio;

impl Room {
    /// Redact the given events in this room because of the given reason.
    ///
    /// Returns `Ok(())` if all the redactions are successful, otherwise
    /// returns the list of events that could not be redacted.
    pub(crate) async fn redact<'a>(
        &self,
        events: &'a [OwnedEventId],
        reason: Option<String>,
    ) -> Result<(), Vec<&'a EventId>> {
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
            return Ok(());
        }

        let events_clone = events.to_owned();
        let matrix_room = matrix_room.clone();
        let handle = spawn_tokio!(async move {
            let mut failed_redactions = Vec::new();

            for (i, event_id) in events_clone.iter().enumerate() {
                match matrix_room.redact(event_id, reason.as_deref(), None).await {
                    Ok(_) => {}
                    Err(error) => {
                        error!("Could not redact event with ID {event_id}: {error}");
                        failed_redactions.push(i);
                    }
                }
            }

            failed_redactions
        });

        let failed_redactions = handle.await.expect("task was not aborted");
        let failed_redactions = failed_redactions
            .into_iter()
            .map(|i| &*events[i])
            .collect::<Vec<_>>();

        if failed_redactions.is_empty() {
            Ok(())
        } else {
            Err(failed_redactions)
        }
    }

    /// Report the given events in this room.
    ///
    /// The events are a list of `(event_id, reason)` tuples.
    ///
    /// Returns `Ok(())` if all the reports are sent successfully, otherwise
    /// returns the list of event IDs that could not be reported.
    pub(crate) async fn report_events<'a>(
        &self,
        events: &'a [(OwnedEventId, Option<String>)],
    ) -> Result<(), Vec<&'a EventId>> {
        let events_clone = events.to_owned();
        let matrix_room = self.matrix_room().clone();
        let handle = spawn_tokio!(async move {
            let futures = events_clone
                .into_iter()
                .map(|(event_id, reason)| matrix_room.report_content(event_id, reason));
            join_all(futures).await
        });

        let mut failed = Vec::new();
        for (index, result) in handle
            .await
            .expect("task was not aborted")
            .iter()
            .enumerate()
        {
            match result {
                Ok(_) => {}
                Err(error) => {
                    error!(
                        "Could not report content with event ID {}: {error}",
                        events[index].0,
                    );
                    failed.push(&*events[index].0);
                }
            }
        }

        if failed.is_empty() {
            Ok(())
        } else {
            Err(failed)
        }
    }

    /// Invite the given users to this room.
    ///
    /// Returns `Ok(())` if all the invites are sent successfully, otherwise
    /// returns the list of users who could not be invited.
    pub(crate) async fn invite<'a>(
        &self,
        user_ids: &'a [OwnedUserId],
    ) -> Result<(), Vec<&'a UserId>> {
        let matrix_room = self.matrix_room();
        if matrix_room.state() != RoomState::Joined {
            error!("Can’t invite users, because this room isn’t a joined room");
            return Ok(());
        }

        let user_ids_clone = user_ids.to_owned();
        let matrix_room = matrix_room.clone();
        let handle = spawn_tokio!(async move {
            let invitations = user_ids_clone
                .iter()
                .map(|user_id| matrix_room.invite_user_by_id(user_id));
            join_all(invitations).await
        });

        let mut failed_invites = Vec::new();
        for (index, result) in handle
            .await
            .expect("task was not aborted")
            .iter()
            .enumerate()
        {
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!("Could not invite user with ID {}: {error}", user_ids[index],);
                    failed_invites.push(&*user_ids[index]);
                }
            }
        }

        if failed_invites.is_empty() {
            Ok(())
        } else {
            Err(failed_invites)
        }
    }

    /// Kick the given users from this room.
    ///
    /// The users are a list of `(user_id, reason)` tuples.
    ///
    /// Returns `Ok(())` if all the kicks are sent successfully, otherwise
    /// returns the list of users who could not be kicked.
    pub(crate) async fn kick<'a>(
        &self,
        users: &'a [(OwnedUserId, Option<String>)],
    ) -> Result<(), Vec<&'a UserId>> {
        let users_clone = users.to_owned();
        let matrix_room = self.matrix_room().clone();
        let handle = spawn_tokio!(async move {
            let futures = users_clone
                .iter()
                .map(|(user_id, reason)| matrix_room.kick_user(user_id, reason.as_deref()));
            join_all(futures).await
        });

        let mut failed_kicks = Vec::new();
        for (index, result) in handle
            .await
            .expect("task was not aborted")
            .iter()
            .enumerate()
        {
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!("Could not kick user with ID {}: {error}", users[index].0);
                    failed_kicks.push(&*users[index].0);
                }
            }
        }

        if failed_kicks.is_empty() {
            Ok(())
        } else {
            Err(failed_kicks)
        }
    }

    /// Ban the given users from this room.
    ///
    /// The users are a list of `(user_id, reason)` tuples.
    ///
    /// Returns `Ok(())` if all the bans are sent successfully, otherwise
    /// returns the list of users who could not be banned.
    pub(crate) async fn ban<'a>(
        &self,
        users: &'a [(OwnedUserId, Option<String>)],
    ) -> Result<(), Vec<&'a UserId>> {
        let users_clone = users.to_owned();
        let matrix_room = self.matrix_room().clone();
        let handle = spawn_tokio!(async move {
            let futures = users_clone
                .iter()
                .map(|(user_id, reason)| matrix_room.ban_user(user_id, reason.as_deref()));
            join_all(futures).await
        });

        let mut failed_bans = Vec::new();
        for (index, result) in handle
            .await
            .expect("task was not aborted")
            .iter()
            .enumerate()
        {
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!("Could not ban user with ID {}: {error}", users[index].0);
                    failed_bans.push(&*users[index].0);
                }
            }
        }

        if failed_bans.is_empty() {
            Ok(())
        } else {
            Err(failed_bans)
        }
    }

    /// Unban the given users from this room.
    ///
    /// The users are a list of `(user_id, reason)` tuples.
    ///
    /// Returns `Ok(())` if all the unbans are sent successfully, otherwise
    /// returns the list of users who could not be unbanned.
    pub(crate) async fn unban<'a>(
        &self,
        users: &'a [(OwnedUserId, Option<String>)],
    ) -> Result<(), Vec<&'a UserId>> {
        let users_clone = users.to_owned();
        let matrix_room = self.matrix_room().clone();
        let handle = spawn_tokio!(async move {
            let futures = users_clone
                .iter()
                .map(|(user_id, reason)| matrix_room.unban_user(user_id, reason.as_deref()));
            join_all(futures).await
        });

        let mut failed_unbans = Vec::new();
        for (index, result) in handle
            .await
            .expect("task was not aborted")
            .iter()
            .enumerate()
        {
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!("Could not unban user with ID {}: {error}", users[index].0);
                    failed_unbans.push(&*users[index].0);
                }
            }
        }

        if failed_unbans.is_empty() {
            Ok(())
        } else {
            Err(failed_unbans)
        }
    }
}
