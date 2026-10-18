// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The pop-out window: one pane of the workspace, in a window of its own.
//!
//! A pop-out is the seat system's oldest rule made literal — a view can be
//! up in more than one seat at once — with the extra seat living on another
//! window's glass rather than in another box of this one. It is a second
//! `App`, not a second copy of anything: the models handed to it are the
//! same objects the main window holds, so what they carry reaches it by
//! existing, and `Studio`'s publishes fan out to it for the rest. Every
//! callback it can make was bound by the same `wire` the main window got,
//! so a drag in a pop-out mutates the one studio and republishes to every
//! window in turn.
//!
//! One window per view, and asking for a view that is already up is a
//! focus, not a second window. Closing it — the platform's own close, or
//! the pane's remove row, which maps to the same teardown — takes it off
//! the studio's list, and that is the whole of it: the main window was
//! never disturbed, and the project is exactly where it was.
//!
//! Two things do not follow it out. A pop-out's transport of the bin's
//! file preview stays on the main window — that preview is the bin's own
//! state and its own window, and a second one would be a second picture
//! of the same instant. And the meters' bars are fed straight to the
//! window the feed was started for; a popped-out meters pane keeps time
//! with everything else but does not dance.

use slint::{CloseRequestResponse, ComponentHandle, LogicalSize};

use crate::host::Shell;
use crate::i18n;
use crate::platform;
use crate::ui::{App, PaneKind};

/// Opens `kind` in a window of its own — and, when a window is already up
/// showing that view, does not open a second one.
pub fn open(shell: &Shell, kind: PaneKind) {
    // One per view. A second of the same kind would show the same scroll,
    // the same playhead and the same lanes — the same picture twice, which
    // is one more window to keep hold of and nothing new to see.
    if let Some((_, up)) = shell
        .studio
        .borrow()
        .popouts
        .iter()
        .find(|(seen, _)| *seen == kind)
    {
        // Where it sits on the screen is the window manager's business —
        // Slint offers no raise — but one left minimised is brought back,
        // which is most of what "bring it forward" was for.
        up.window().set_minimized(false);
        return;
    }

    let app = match App::new() {
        Ok(app) => app,
        Err(error) => {
            log::error!("popout: could not create window: {error}");
            return;
        }
    };

    // The platform's own decorations: a bar carrying the view's name and
    // the close button every window must have. `no-frame` reads
    // own-window-buttons, and the strip that button would sit in is not
    // built in a pop-out at all — see app.slint.
    app.set_own_window_buttons(false);
    app.set_macos(platform::MACOS);
    app.set_phone(false);
    app.set_on_start(false);
    app.set_popout(true);
    app.set_popout_kind(kind);

    let (w, h, label) = shape(kind);
    // The title is not set from here: `slint::Window` has no `set_title`,
    // and does not need one — app.slint binds it to the kind set above,
    // through the same label the pane switcher reads.
    app.window().set_size(LogicalSize::new(w, h));

    // The same bindings the main window got, so every verb in every pane
    // works here and lands in the same studio. See `wire`.
    crate::wire(&app, shell);

    // Both roads out land in the same place: the platform's close — the
    // bar's button, Alt+F4, the window manager — and the pane's own remove
    // row, which the tree maps to `popout-close`. Either way the entry
    // goes, and the last strong handle to the window with it.
    app.on_popout_close(move || forget(kind));
    app.window().on_close_requested(move || {
        forget(kind);
        log::info!("popout: closed {}", shape(kind).2);
        CloseRequestResponse::HideWindow
    });

    if let Err(error) = app.show() {
        log::error!("popout: could not show window: {error}");
        return;
    }
    log::info!("popout: opened {label}");
    shell.studio.borrow_mut().popouts.push((kind, app));
}

/// Drops the pop-out showing `kind`, if it is still up.
///
/// The window is not hidden first: taking the entry off the list is the
/// last strong handle, and a window that is dropped is a window that is
/// closed. Doing this from inside that window's own callback is the
/// ordinary case — the remove row — and Slint holds the component for the
/// duration of the dispatch, so the drop lands after it returns.
fn forget(kind: PaneKind) {
    Shell::with(|shell, _| {
        shell
            .studio
            .borrow_mut()
            .popouts
            .retain(|(seen, _)| *seen != kind);
    });
}

/// A starting size for each view, and the name its window wears.
///
/// The sizes are a first guess with the view's shape in it, not a policy:
/// the timeline wants to be wide and the inspector wants to be tall, and
/// the rest is the person's to move from wherever the OS put the window. The
/// names are the same keys the pane switcher's rows read, so a view is
/// called one thing everywhere it can be seen.
fn shape(kind: PaneKind) -> (f32, f32, String) {
    let (w, h, key) = match kind {
        PaneKind::Media => (460.0, 720.0, "paneSwitcher.library"),
        PaneKind::Preview => (960.0, 680.0, "common.preview"),
        PaneKind::Inspector => (420.0, 720.0, "paneSwitcher.inspector"),
        PaneKind::Timeline => (1280.0, 420.0, "paneSwitcher.timeline"),
        PaneKind::Keyframes => (880.0, 400.0, "common.keyframes"),
    };
    (w, h, i18n::t(key))
}
