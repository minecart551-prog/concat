// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The workspace's arrangement: a tree of splits and views, walked flat
//! for Slint, which has no recursion.

use serde::{Deserialize, Serialize};

use crate::ui::{DockDivider, DockSide, PaneKind, SeatBox};

pub const SEAT_MIN_W: f32 = 240.0;
pub const SEAT_MIN_H: f32 = 140.0;
/// The floor that holds where the one above cannot: enough of a seat left to
/// find its corner button with, which is the only way back from a layout that
/// has been squeezed too far.
pub const SEAT_MIN_GRAB: f32 = 64.0;
/// The gutter between the halves of a split, and the margin round the lot.
pub const SEAT_GAP: f32 = 8.0;

pub enum Dock {
    /// One view, filling its box.
    Leaf(PaneKind),
    /// Two nodes and the gutter between them. `columns` means side by side,
    /// which is a vertical gutter dragged left and right; `ratio` is the share
    /// of the usable extent — the box less the gutter — that the first takes.
    Split {
        columns: bool,
        ratio: f32,
        first: Box<Dock>,
        second: Box<Dock>,
    },
}

impl Dock {
    pub fn leaf(kind: PaneKind) -> Box<Dock> {
        Box::new(Dock::Leaf(kind))
    }

    /// The node a path names, following each step into the first branch or the
    /// second. A path that runs past a leaf stops there, which cannot happen
    /// for a path this module produced.
    pub fn at(&self, path: &[bool]) -> &Dock {
        let Some((step, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Dock::Split { first, second, .. } => {
                if *step {
                    second.at(rest)
                } else {
                    first.at(rest)
                }
            }
            leaf => leaf,
        }
    }

    pub fn at_mut(&mut self, path: &[bool]) -> &mut Dock {
        let Some((step, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Dock::Split { first, second, .. } => {
                if *step {
                    second.at_mut(rest)
                } else {
                    first.at_mut(rest)
                }
            }
            leaf => leaf,
        }
    }

    /// The path to the nth leaf, counted in the same walk `lay_out` uses — so
    /// the number a seat carries and the number a drop reports are the same
    /// number. Paths rather than indices for the editing below, because an
    /// index is invalidated by a change elsewhere in the tree and a path is
    /// only invalidated by a change on its own way down.
    pub fn leaf_path(&self, index: usize) -> Option<Vec<bool>> {
        fn walk(node: &Dock, want: usize, seen: &mut usize, path: &mut Vec<bool>) -> bool {
            match node {
                Dock::Leaf(_) => {
                    if *seen == want {
                        return true;
                    }
                    *seen += 1;
                    false
                }
                Dock::Split { first, second, .. } => {
                    path.push(false);
                    if walk(first, want, seen, path) {
                        return true;
                    }
                    path.pop();
                    path.push(true);
                    if walk(second, want, seen, path) {
                        return true;
                    }
                    path.pop();
                    false
                }
            }
        }
        let (mut seen, mut path) = (0, Vec::new());
        walk(self, index, &mut seen, &mut path).then_some(path)
    }

    /// The same for the nth split — parent before children, which is the order
    /// `lay_out` numbers the gutters in.
    pub fn split_path(&self, index: usize) -> Option<Vec<bool>> {
        fn walk(node: &Dock, want: usize, seen: &mut usize, path: &mut Vec<bool>) -> bool {
            let Dock::Split { first, second, .. } = node else {
                return false;
            };
            if *seen == want {
                return true;
            }
            *seen += 1;
            path.push(false);
            if walk(first, want, seen, path) {
                return true;
            }
            path.pop();
            path.push(true);
            if walk(second, want, seen, path) {
                return true;
            }
            path.pop();
            false
        }
        let (mut seen, mut path) = (0, Vec::new());
        walk(self, index, &mut seen, &mut path).then_some(path)
    }

    /// Whether a view of this kind is anywhere in the tree.
    pub fn contains(&self, kind: PaneKind) -> bool {
        match self {
            Dock::Leaf(held) => *held == kind,
            Dock::Split { first, second, .. } => first.contains(kind) || second.contains(kind),
        }
    }

    pub fn kind_at(&self, index: usize) -> Option<PaneKind> {
        match self.at(&self.leaf_path(index)?) {
            Dock::Leaf(kind) => Some(*kind),
            _ => None,
        }
    }

    /// Turn one leaf into a split, with a new view on the named side of it.
    /// Half and half: the seat being split is the only thing that knows how
    /// much room there is, and half of it is the answer that needs no rule.
    pub fn split_leaf(&mut self, path: &[bool], kind: PaneKind, side: DockSide) {
        let node = self.at_mut(path);
        let existing = Box::new(std::mem::replace(node, Dock::Leaf(kind)));
        let (columns, first, second) = match side {
            DockSide::Left => (true, Dock::leaf(kind), existing),
            DockSide::Right => (true, existing, Dock::leaf(kind)),
            DockSide::Top => (false, Dock::leaf(kind), existing),
            _ => (false, existing, Dock::leaf(kind)),
        };
        *node = Dock::Split {
            columns,
            ratio: 0.5,
            first,
            second,
        };
    }

    /// Take a leaf out, and let its sibling take the space — the split it was
    /// half of stops existing. A path with nothing above it is the whole tree,
    /// which cannot be removed: a window with no seats has nowhere to put
    /// anything back.
    pub fn remove_leaf(&mut self, path: &[bool]) {
        let Some((last, above)) = path.split_last() else {
            return;
        };
        let parent = self.at_mut(above);
        let survivor = match parent {
            Dock::Split { first, second, .. } => {
                let keep = if *last { first } else { second };
                std::mem::replace(keep.as_mut(), Dock::Leaf(PaneKind::Media))
            }
            _ => return,
        };
        *parent = survivor;
    }

    /// The arrangement as it is remembered: the same tree, in the names the
    /// settings file can hold. See [`SavedDock`].
    pub fn as_saved(&self) -> SavedDock {
        match self {
            Dock::Leaf(kind) => SavedDock::Leaf {
                pane: slug(*kind).to_owned(),
            },
            Dock::Split {
                columns,
                ratio,
                first,
                second,
            } => SavedDock::Split {
                columns: *columns,
                ratio: *ratio,
                first: Box::new(first.as_saved()),
                second: Box::new(second.as_saved()),
            },
        }
    }
}

/// The arrangement, as the settings file remembers it between runs: the same
/// tree as [`Dock`], in a form serde can write and a person can read. A leaf
/// names its view by the same slug the drag payloads use ("media",
/// "timeline", ...), so the file and what a drop says speak one vocabulary.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SavedDock {
    Leaf {
        pane: String,
    },
    Split {
        columns: bool,
        ratio: f32,
        first: Box<SavedDock>,
        second: Box<SavedDock>,
    },
}

/// How many levels a remembered arrangement may have, from the window's
/// corner down to a view. The editor's own is four; anything past this is a
/// file written by hand or by a bug, and what lies beyond the cut becomes the
/// library rather than being trusted to lay out — a walk that recurses
/// forever is a crash, not a layout.
const MAX_SAVED_DEPTH: usize = 8;
/// The share a remembered split's halves may fall to. The window's own
/// dividers hold a floor of their own while they are dragged (see the seat
/// minimums); this is the same idea for a file nobody clamped.
const MIN_SAVED_RATIO: f32 = 0.1;

impl SavedDock {
    /// The arrangement this remembers, with a hand-edited file's excesses
    /// taken off: a ratio pushed to either end, a view the app does not have
    /// (any more), a tree deeper than the window could draw. A settings file
    /// is only ever as trustworthy as the last hand that had it, and the
    /// rule everywhere else in prefs is that a bad value is the default and
    /// never an error.
    pub fn to_dock(&self) -> Dock {
        self.to_dock_at(0)
    }

    fn to_dock_at(&self, depth: usize) -> Dock {
        // `depth` counts the splits above this node; the node itself is a
        // level too, which is the one the cap is a count of.
        if depth + 1 >= MAX_SAVED_DEPTH {
            return Dock::Leaf(PaneKind::Media);
        }
        match self {
            SavedDock::Leaf { pane } => Dock::Leaf(kind_of(pane)),
            SavedDock::Split {
                columns,
                ratio,
                first,
                second,
            } => Dock::Split {
                columns: *columns,
                ratio: ratio.clamp(MIN_SAVED_RATIO, 1.0 - MIN_SAVED_RATIO),
                first: Box::new(first.to_dock_at(depth + 1)),
                second: Box::new(second.to_dock_at(depth + 1)),
            },
        }
    }
}

/// The name a view goes by in a saved arrangement — and in a drag payload,
/// which is where the vocabulary comes from: `Panes.id` in pane-switcher.slint
/// writes the same five words.
fn slug(kind: PaneKind) -> &'static str {
    match kind {
        PaneKind::Media => "media",
        PaneKind::Preview => "preview",
        PaneKind::Inspector => "inspector",
        PaneKind::Keyframes => "keyframes",
        PaneKind::Timeline => "timeline",
    }
}

/// The view a saved name asks for. A name this app does not know — one left
/// behind by a version that had another view, say — is the library: the seat
/// that can always show something, and the one the default layout starts with.
fn kind_of(name: &str) -> PaneKind {
    match name {
        "preview" => PaneKind::Preview,
        "inspector" => PaneKind::Inspector,
        "keyframes" => PaneKind::Keyframes,
        "timeline" => PaneKind::Timeline,
        _ => PaneKind::Media,
    }
}

/// The tree walked flat: a box per view, a box per gutter.
#[derive(Default)]
pub struct DockLayout {
    pub seats: Vec<SeatBox>,
    pub dividers: Vec<DockDivider>,
    /// The usable extent of each split — its box less the gutter — in the same
    /// order as `dividers`. A divider drag is a delta in pixels and a ratio is
    /// a fraction, and this is what converts between them.
    pub extents: Vec<f32>,
}

pub fn lay_out(node: &Dock, (x, y, w, h): (f32, f32, f32, f32), out: &mut DockLayout) {
    match node {
        Dock::Leaf(kind) => out.seats.push(SeatBox {
            index: out.seats.len() as i32,
            kind: *kind,
            x,
            y,
            width: w.max(0.0),
            height: h.max(0.0),
        }),
        Dock::Split {
            columns,
            ratio,
            first,
            second,
        } => {
            // The slot is taken before the children are walked, so a gutter's
            // number is its split's position in a parent-first walk — which is
            // what `split_path` counts and what a drag reports back.
            let index = out.dividers.len();
            out.dividers.push(DockDivider::default());
            out.extents.push(0.0);

            let along = if *columns { w } else { h };
            let usable = (along - SEAT_GAP).max(0.0);
            let head = (usable * ratio).clamp(0.0, usable);
            out.extents[index] = usable;
            out.dividers[index] = if *columns {
                DockDivider {
                    index: index as i32,
                    columns: true,
                    x: x + head,
                    y,
                    width: SEAT_GAP,
                    height: h.max(0.0),
                }
            } else {
                DockDivider {
                    index: index as i32,
                    columns: false,
                    x,
                    y: y + head,
                    width: w.max(0.0),
                    height: SEAT_GAP,
                }
            };

            if *columns {
                lay_out(first, (x, y, head, h), out);
                lay_out(second, (x + head + SEAT_GAP, y, usable - head, h), out);
            } else {
                lay_out(first, (x, y, w, head), out);
                lay_out(second, (x, y + head + SEAT_GAP, w, usable - head), out);
            }
        }
    }
}

/// The arrangement the editor opens with: the inspector a column down the
/// whole right side, and in the rest the library and the monitor across the
/// top and the timeline under both of them. The inspector is the tallest
/// thing in the window - a stack of sections that would otherwise scroll -
/// and the timeline gives up its right end to it, which is the end with
/// nothing on it yet. About 0.22 of the width to the inspector and 0.25 to
/// the library, which is 0.32 of what is left once the inspector has had
/// its share; the timeline takes the lower 0.4 of the height.
pub fn default_dock() -> Dock {
    Dock::Split {
        columns: true,
        ratio: 0.78,
        first: Box::new(Dock::Split {
            columns: false,
            ratio: 0.6,
            first: Box::new(Dock::Split {
                columns: true,
                ratio: 0.32,
                first: Dock::leaf(PaneKind::Media),
                second: Dock::leaf(PaneKind::Preview),
            }),
            // The timeline, whose VU meters are a column at its own right
            // edge (see vu-meter.slint) rather than a seat of their own.
            second: Dock::leaf(PaneKind::Timeline),
        }),
        second: Dock::leaf(PaneKind::Inspector),
    }
}

/// The top edge of a row, measured down the stack from the first lane.
/// The dock a narrow window shows: the monitor over the timeline, and
/// nothing beside either. The library and the inspector are a switch away
/// on each seat - see `PaneSwitcher` - which on a phone is how they are
/// reached in every editor: one at a time, in the space the picture had.
pub fn compact_dock() -> Dock {
    Dock::Split {
        columns: false,
        ratio: 0.5,
        first: Dock::leaf(PaneKind::Preview),
        second: Dock::leaf(PaneKind::Timeline),
    }
}

pub fn row_top(heights: &[f32], row: i32) -> f32 {
    heights.iter().take(row.max(0) as usize).sum()
}

/// The row a point down the stack falls in, clamped to the stack.
pub fn row_at(heights: &[f32], y: f32) -> i32 {
    let mut top = 0.0;
    for (row, height) in heights.iter().enumerate() {
        top += height;
        if y < top {
            return row as i32;
        }
    }
    (heights.len() as i32 - 1).max(0)
}

/// The row whose top edge is nearest a point down the stack.
///
/// What a *move* wants, and not the same question as `row_at`: dragging a clip
/// halfway into the lane below should put it there, which is the rounding the
/// old `delta / lane-height` did for free back when every lane was one height.
pub fn nearest_row(heights: &[f32], y: f32) -> i32 {
    let (mut top, mut best, mut best_distance) = (0.0_f32, 0_i32, f32::MAX);
    for (row, height) in heights.iter().enumerate() {
        let distance = (top - y).abs();
        if distance < best_distance {
            best_distance = distance;
            best = row as i32;
        }
        top += height;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arrangement the editor opens with survives the settings file —
    /// out through serde and back in as the same tree.
    #[test]
    fn an_arrangement_comes_back_from_the_file_as_it_was() {
        let original = default_dock();
        let written = serde_json::to_string(&original.as_saved()).expect("encodes");
        let read: SavedDock = serde_json::from_str(&written).expect("decodes");
        let back = read.to_dock();

        // Same shape, said with the thing the file said: re-encoding the
        // round-tripped tree gives the same text.
        assert_eq!(
            serde_json::to_string(&back.as_saved()).expect("encodes"),
            written
        );
        // And the views are where they were.
        for kind in [
            PaneKind::Media,
            PaneKind::Preview,
            PaneKind::Timeline,
            PaneKind::Inspector,
        ] {
            assert!(back.contains(kind), "{kind:?} survived");
        }
    }

    /// A ratio a hand pushed past either end is pulled back to a share both
    /// halves can be used at, rather than trusted to leave a seat nothing.
    #[test]
    fn a_ratio_the_file_pushed_too_far_is_pulled_back_in() {
        let saved = SavedDock::Split {
            columns: true,
            ratio: 9.0,
            first: Box::new(SavedDock::Leaf {
                pane: "timeline".to_owned(),
            }),
            second: Box::new(SavedDock::Leaf {
                pane: "media".to_owned(),
            }),
        };
        let Dock::Split { ratio, .. } = saved.to_dock() else {
            panic!("still a split");
        };
        assert_eq!(ratio, 1.0 - MIN_SAVED_RATIO);

        let saved = SavedDock::Split {
            columns: true,
            ratio: -3.0,
            first: Box::new(SavedDock::Leaf {
                pane: "timeline".to_owned(),
            }),
            second: Box::new(SavedDock::Leaf {
                pane: "media".to_owned(),
            }),
        };
        let Dock::Split { ratio, .. } = saved.to_dock() else {
            panic!("still a split");
        };
        assert_eq!(ratio, MIN_SAVED_RATIO);
    }

    /// A view the app does not have — one left behind by a version that had
    /// another — comes up as the library, which can always show something.
    #[test]
    fn a_view_the_app_does_not_have_is_the_library() {
        let saved = SavedDock::Leaf {
            pane: "scopes".to_owned(),
        };
        assert!(matches!(saved.to_dock(), Dock::Leaf(PaneKind::Media)));
    }

    /// A tree deeper than the window could draw is cut down at the depth,
    /// and what is left still lays out: the file does not get the last word.
    #[test]
    fn a_tree_too_deep_for_the_window_is_cut_down_to_the_depth() {
        let mut saved = SavedDock::Leaf {
            pane: "timeline".to_owned(),
        };
        for _ in 0..40 {
            saved = SavedDock::Split {
                columns: true,
                ratio: 0.5,
                first: Box::new(saved),
                second: Box::new(SavedDock::Leaf {
                    pane: "media".to_owned(),
                }),
            };
        }
        let dock = saved.to_dock();
        // Cut at the depth: the tree is at most MAX_SAVED_DEPTH deep, which
        // is short of the forty the file asked for.
        fn depth(node: &Dock) -> usize {
            match node {
                Dock::Leaf(_) => 1,
                Dock::Split { first, second, .. } => 1 + depth(first).max(depth(second)),
            }
        }
        assert!(depth(&dock) <= MAX_SAVED_DEPTH);
        let mut out = DockLayout::default();
        lay_out(&dock, (0.0, 0.0, 1920.0, 1080.0), &mut out);
        assert!(!out.seats.is_empty());
    }
}
