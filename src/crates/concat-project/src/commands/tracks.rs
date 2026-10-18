// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Lanes: adding, removing and muting or hiding them.
//!
//! One arm per command, exactly as [`super::apply`] routes them here;
//! everything these arms share lives in the parent module.

use super::*;

/// Applies one of this module's commands. Any other is a routing error.
pub(super) fn apply(
    project: &mut Project,
    mint: &mut IdMint,
    command: Command,
) -> Result<Outcome, CommandError> {
    match command {
        Command::AddTrack { bottom } => {
            let timeline = project.active_mut();
            let id = mint.next("t");
            let track = Track {
                id: id.clone(),
                visible: true,
                muted: false,
                size: None,
                height: None,
                extra: Default::default(),
            };
            // The stack is stored bottom first: the last track is the lane
            // drawn on top.
            if bottom {
                timeline.tracks.insert(0, track);
            } else {
                timeline.tracks.push(track);
            }
            Ok(Outcome {
                created_id: Some(id),
                applied: true,
            })
        }

        Command::RemoveTrack { track_id } => {
            let timeline = project.active_mut();
            if timeline.tracks.len() <= 1 {
                return Err(CommandError::LastTrack);
            }
            let track_count = timeline.tracks.len();
            timeline.tracks.retain(|track| track.id != track_id);
            timeline.clips.retain(|clip| clip.track_id != track_id);
            // Clips only ever sit on existing tracks, so an unknown id - the
            // tolerated no-op - removes neither.
            let applied = timeline.tracks.len() != track_count;
            Ok(Outcome {
                created_id: None,
                applied,
            })
        }

        Command::SetTrackFlag {
            track_id,
            flag,
            value,
        } => {
            let timeline = project.active_mut();
            let applied = timeline
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
                .is_some_and(|track| match flag {
                    TrackFlag::Visible => assign(&mut track.visible, value),
                    TrackFlag::Muted => assign(&mut track.muted, value),
                });
            Ok(Outcome {
                created_id: None,
                applied,
            })
        }

        // View state: a lane's layout is saved with the project so a
        // reopened one keeps it, and is never recorded in undo - see
        // `Command::is_view_state`.
        Command::SetTrackSize {
            track_id,
            size,
            height,
        } => {
            let timeline = project.active_mut();
            // An unknown id is the tolerated no-op, as SetTrackFlag's is.
            let Some(track) = timeline
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id)
            else {
                return Ok(Outcome {
                    created_id: None,
                    applied: false,
                });
            };
            // A height belongs only to a custom size - a preset is its own
            // height - and only a finite one above the floor: a document is
            // read as well as written, and `NaN.clamp()` is NaN. Both the
            // floor and the ceiling for a dragged height are the UI's
            // (LANE_MIN, LANE_MAX); they are applied there, on the way in.
            let height = (size == "custom")
                .then_some(height)
                .flatten()
                .filter(|height| height.is_finite() && *height > 0.0);
            let changed = track.size.as_deref() != Some(size.as_str()) || track.height != height;
            track.size = Some(size);
            track.height = height;
            Ok(Outcome {
                created_id: None,
                applied: changed,
            })
        }

        _ => unreachable!("commands::apply routes only this module's commands here"),
    }
}
