//! The split tree of a tab (PLAN §5.3, Sprint 4): a binary tree whose leaves are panes.
//!
//! A split divides its area along an axis (`Horizontal`: side by side, `Vertical`: stacked) at
//! a ratio, between a first and a second child. Every operation keeps the tree valid: a split
//! always has two children, ratios stay within [`MIN_RATIO`]..=[`MAX_RATIO`], and each pane id
//! appears once. Geometry is computed in the unit square; the app scales it to the tab's area.

use serde::{Deserialize, Serialize};

/// A pane's id: its terminal session's id in the app.
pub type PaneId = i32;

/// Smallest share of a split either child keeps.
pub const MIN_RATIO: f64 = 0.05;
/// Largest share of a split either child keeps.
pub const MAX_RATIO: f64 = 0.95;

/// How a split divides its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Side by side: the first child on the left.
    Horizontal,
    /// Stacked: the first child on top.
    Vertical,
}

/// A direction to look for a neighbour, or to move a divider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Toward x = 0.
    Left,
    /// Toward x = 1.
    Right,
    /// Toward y = 0.
    Up,
    /// Toward y = 1.
    Down,
}

impl Direction {
    /// The axis this direction moves along.
    #[must_use]
    pub const fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Up | Self::Down => Axis::Vertical,
        }
    }

    /// Whether it moves toward 1 (right or down).
    #[must_use]
    pub const fn forward(self) -> bool {
        matches!(self, Self::Right | Self::Down)
    }
}

/// A node of the tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Node {
    /// A leaf.
    Pane {
        /// The pane.
        pane: PaneId,
    },
    /// Two children along an axis.
    Split {
        /// Side by side or stacked.
        split: Axis,
        /// The first child's share, `MIN_RATIO..=MAX_RATIO`.
        ratio: f64,
        /// Left or top.
        first: Box<Node>,
        /// Right or bottom.
        second: Box<Node>,
    },
}

/// A rectangle in the unit square.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    /// The whole area.
    pub const UNIT: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    fn split(self, axis: Axis, ratio: f64) -> (Self, Self) {
        match axis {
            Axis::Horizontal => {
                let first = self.width * ratio;
                (
                    Self {
                        width: first,
                        ..self
                    },
                    Self {
                        x: self.x + first,
                        width: self.width - first,
                        ..self
                    },
                )
            }
            Axis::Vertical => {
                let first = self.height * ratio;
                (
                    Self {
                        height: first,
                        ..self
                    },
                    Self {
                        y: self.y + first,
                        height: self.height - first,
                        ..self
                    },
                )
            }
        }
    }

    fn center(self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// Where a pane is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaneRect {
    /// The pane.
    pub pane: PaneId,
    /// Its area.
    #[serde(flatten)]
    pub rect: Rect,
}

/// A divider between the two children of a split, for dragging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Divider {
    /// The split, as the path from the root: `false` for first, `true` for second.
    pub path: Vec<bool>,
    /// The split's axis (a horizontal split has a vertical divider line).
    pub axis: Axis,
    /// The split's whole area.
    pub area: Rect,
    /// The current ratio.
    pub ratio: f64,
}

/// The split tree of one tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Layout {
    root: Node,
}

fn clamp_ratio(ratio: f64) -> f64 {
    if ratio.is_finite() {
        ratio.clamp(MIN_RATIO, MAX_RATIO)
    } else {
        0.5
    }
}

impl Layout {
    /// One pane filling the tab.
    #[must_use]
    pub const fn single(pane: PaneId) -> Self {
        Self {
            root: Node::Pane { pane },
        }
    }

    /// A layout from a tree read from a file or from QML. Fails when a pane id repeats; ratios
    /// are clamped.
    ///
    /// # Errors
    ///
    /// A short reason when the tree is not usable.
    pub fn from_node(mut root: Node) -> Result<Self, String> {
        fn fix(node: &mut Node) {
            if let Node::Split {
                ratio,
                first,
                second,
                ..
            } = node
            {
                *ratio = clamp_ratio(*ratio);
                fix(first);
                fix(second);
            }
        }
        fix(&mut root);
        let layout = Self { root };
        let mut panes = layout.panes();
        let count = panes.len();
        panes.sort_unstable();
        panes.dedup();
        if panes.len() != count {
            return Err("a pane appears twice in the layout".to_owned());
        }
        Ok(layout)
    }

    /// The tree.
    #[must_use]
    pub const fn root(&self) -> &Node {
        &self.root
    }

    /// Every pane, in reading order (left to right, top to bottom within each split).
    #[must_use]
    pub fn panes(&self) -> Vec<PaneId> {
        fn walk(node: &Node, out: &mut Vec<PaneId>) {
            match node {
                Node::Pane { pane } => out.push(*pane),
                Node::Split { first, second, .. } => {
                    walk(first, out);
                    walk(second, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    /// Whether `pane` is in the tree.
    #[must_use]
    pub fn contains(&self, pane: PaneId) -> bool {
        self.panes().contains(&pane)
    }

    /// Splits `target`: `new` goes after it (right or below) when `after`, else before.
    /// Returns false when `target` isn't in the tree or `new` already is.
    pub fn split(&mut self, target: PaneId, axis: Axis, new: PaneId, after: bool) -> bool {
        if self.contains(new) {
            return false;
        }
        fn walk(node: &mut Node, target: PaneId, axis: Axis, new: PaneId, after: bool) -> bool {
            match node {
                Node::Pane { pane } if *pane == target => {
                    let old = Node::Pane { pane: target };
                    let added = Node::Pane { pane: new };
                    let (first, second) = if after { (old, added) } else { (added, old) };
                    *node = Node::Split {
                        split: axis,
                        ratio: 0.5,
                        first: Box::new(first),
                        second: Box::new(second),
                    };
                    true
                }
                Node::Pane { .. } => false,
                Node::Split { first, second, .. } => {
                    walk(first, target, axis, new, after) || walk(second, target, axis, new, after)
                }
            }
        }
        walk(&mut self.root, target, axis, new, after)
    }

    /// Removes `target`; its sibling takes the parent split's place. Returns the pane that
    /// should get the focus (the nearest pane of the sibling), or `None` when `target` isn't in
    /// the tree or is the last pane (a tab always keeps one).
    pub fn close(&mut self, target: PaneId) -> Option<PaneId> {
        fn first_pane(node: &Node) -> PaneId {
            match node {
                Node::Pane { pane } => *pane,
                Node::Split { first, .. } => first_pane(first),
            }
        }
        fn last_pane(node: &Node) -> PaneId {
            match node {
                Node::Pane { pane } => *pane,
                Node::Split { second, .. } => last_pane(second),
            }
        }
        fn walk(node: &mut Node, target: PaneId) -> Option<PaneId> {
            let Node::Split { first, second, .. } = node else {
                return None;
            };
            let hit_first = matches!(**first, Node::Pane { pane } if pane == target);
            let hit_second = matches!(**second, Node::Pane { pane } if pane == target);
            if hit_first || hit_second {
                let sibling = if hit_first {
                    std::mem::replace(&mut **second, Node::Pane { pane: 0 })
                } else {
                    std::mem::replace(&mut **first, Node::Pane { pane: 0 })
                };
                // The pane next to the one that went away.
                let focus = if hit_first {
                    first_pane(&sibling)
                } else {
                    last_pane(&sibling)
                };
                *node = sibling;
                return Some(focus);
            }
            walk(first, target).or_else(|| walk(second, target))
        }
        walk(&mut self.root, target)
    }

    /// The area of every pane.
    #[must_use]
    pub fn rects(&self) -> Vec<PaneRect> {
        fn walk(node: &Node, area: Rect, out: &mut Vec<PaneRect>) {
            match node {
                Node::Pane { pane } => out.push(PaneRect {
                    pane: *pane,
                    rect: area,
                }),
                Node::Split {
                    split,
                    ratio,
                    first,
                    second,
                } => {
                    let (a, b) = area.split(*split, *ratio);
                    walk(first, a, out);
                    walk(second, b, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, Rect::UNIT, &mut out);
        out
    }

    /// Every divider, outermost first.
    #[must_use]
    pub fn dividers(&self) -> Vec<Divider> {
        fn walk(node: &Node, area: Rect, path: &mut Vec<bool>, out: &mut Vec<Divider>) {
            if let Node::Split {
                split,
                ratio,
                first,
                second,
            } = node
            {
                out.push(Divider {
                    path: path.clone(),
                    axis: *split,
                    area,
                    ratio: *ratio,
                });
                let (a, b) = area.split(*split, *ratio);
                path.push(false);
                walk(first, a, path, out);
                path.pop();
                path.push(true);
                walk(second, b, path, out);
                path.pop();
            }
        }
        let mut out = Vec::new();
        walk(&self.root, Rect::UNIT, &mut Vec::new(), &mut out);
        out
    }

    /// Sets the ratio of the split at `path` (clamped). Returns false for a path that doesn't
    /// lead to a split.
    pub fn set_ratio(&mut self, path: &[bool], value: f64) -> bool {
        let mut node = &mut self.root;
        for &second in path {
            match node {
                Node::Split {
                    first,
                    second: other,
                    ..
                } => node = if second { other } else { first },
                Node::Pane { .. } => return false,
            }
        }
        match node {
            Node::Split { ratio, .. } => {
                *ratio = clamp_ratio(value);
                true
            }
            Node::Pane { .. } => false,
        }
    }

    /// The pane next to `from` in `direction`: among the panes that touch its edge on that side,
    /// the one that overlaps it most along the edge (then the closest to its center).
    #[must_use]
    pub fn neighbor(&self, from: PaneId, direction: Direction) -> Option<PaneId> {
        const EPS: f64 = 1e-9;
        let rects = self.rects();
        let origin = rects.iter().find(|r| r.pane == from)?.rect;
        let (cx, cy) = origin.center();
        let overlap = |a0: f64, a1: f64, b0: f64, b1: f64| (a1.min(b1) - a0.max(b0)).max(0.0);
        rects
            .iter()
            .filter(|r| r.pane != from)
            .filter_map(|r| {
                let rect = r.rect;
                let touches = match direction {
                    Direction::Left => (rect.x + rect.width - origin.x).abs() < EPS,
                    Direction::Right => (origin.x + origin.width - rect.x).abs() < EPS,
                    Direction::Up => (rect.y + rect.height - origin.y).abs() < EPS,
                    Direction::Down => (origin.y + origin.height - rect.y).abs() < EPS,
                };
                if !touches {
                    return None;
                }
                let shared = match direction.axis() {
                    Axis::Horizontal => overlap(
                        origin.y,
                        origin.y + origin.height,
                        rect.y,
                        rect.y + rect.height,
                    ),
                    Axis::Vertical => overlap(
                        origin.x,
                        origin.x + origin.width,
                        rect.x,
                        rect.x + rect.width,
                    ),
                };
                if shared <= EPS {
                    return None;
                }
                let (rx, ry) = rect.center();
                let distance = (rx - cx).abs() + (ry - cy).abs();
                Some((r.pane, shared, distance))
            })
            // Most overlap wins, then the closest; on a tie the first in reading order.
            .fold(None::<(PaneId, f64, f64)>, |best, candidate| match best {
                Some(best)
                    if best.1 > candidate.1 + EPS
                        || ((best.1 - candidate.1).abs() <= EPS && best.2 <= candidate.2 + EPS) =>
                {
                    Some(best)
                }
                _ => Some(candidate),
            })
            .map(|(pane, _, _)| pane)
    }

    /// Moves the divider on `pane`'s `direction` side by `step` of the whole area (toward
    /// `direction`): the pane grows that way. Returns false when there is no divider there.
    pub fn resize(&mut self, pane: PaneId, direction: Direction, step: f64) -> bool {
        // The innermost split along the direction's axis where the pane is in the first child
        // (for right and down) or in the second (for left and up).
        fn find(
            node: &Node,
            pane: PaneId,
            direction: Direction,
            area: Rect,
            path: &mut Vec<bool>,
            best: &mut Option<(Vec<bool>, Rect)>,
        ) -> bool {
            match node {
                Node::Pane { pane: id } => *id == pane,
                Node::Split {
                    split,
                    ratio,
                    first,
                    second,
                } => {
                    let (a, b) = area.split(*split, *ratio);
                    path.push(false);
                    let in_first = find(first, pane, direction, a, path, best);
                    path.pop();
                    if in_first {
                        if *split == direction.axis() && direction.forward() && best.is_none() {
                            *best = Some((path.clone(), area));
                        }
                        return true;
                    }
                    path.push(true);
                    let in_second = find(second, pane, direction, b, path, best);
                    path.pop();
                    if in_second
                        && *split == direction.axis()
                        && !direction.forward()
                        && best.is_none()
                    {
                        *best = Some((path.clone(), area));
                    }
                    in_second
                }
            }
        }
        let mut best = None;
        if !find(
            &self.root,
            pane,
            direction,
            Rect::UNIT,
            &mut Vec::new(),
            &mut best,
        ) {
            return false;
        }
        let Some((path, area)) = best else {
            return false;
        };
        let size = match direction.axis() {
            Axis::Horizontal => area.width,
            Axis::Vertical => area.height,
        };
        if size <= 0.0 {
            return false;
        }
        let delta = step / size * if direction.forward() { 1.0 } else { -1.0 };
        let current = self
            .dividers()
            .into_iter()
            .find(|divider| divider.path == path)
            .map_or(0.5, |divider| divider.ratio);
        self.set_ratio(&path, current + delta)
    }

    /// Exchanges two panes' places. Returns false unless both are in the tree.
    pub fn swap(&mut self, a: PaneId, b: PaneId) -> bool {
        if a == b || !self.contains(a) || !self.contains(b) {
            return false;
        }
        fn walk(node: &mut Node, a: PaneId, b: PaneId) {
            match node {
                Node::Pane { pane } if *pane == a => *pane = b,
                Node::Pane { pane } if *pane == b => *pane = a,
                Node::Pane { .. } => {}
                Node::Split { first, second, .. } => {
                    walk(first, a, b);
                    walk(second, a, b);
                }
            }
        }
        walk(&mut self.root, a, b);
        true
    }

    /// Gives every split an even share (each child gets half), keeping the structure.
    pub fn equalize(&mut self) {
        fn walk(node: &mut Node) {
            if let Node::Split {
                ratio,
                first,
                second,
                ..
            } = node
            {
                *ratio = 0.5;
                walk(first);
                walk(second);
            }
        }
        walk(&mut self.root);
    }

    /// The same tree with every pane id replaced by `map(id)`.
    #[must_use]
    pub fn map_panes(&self, mut map: impl FnMut(PaneId) -> PaneId) -> Self {
        fn walk(node: &Node, map: &mut impl FnMut(PaneId) -> PaneId) -> Node {
            match node {
                Node::Pane { pane } => Node::Pane { pane: map(*pane) },
                Node::Split {
                    split,
                    ratio,
                    first,
                    second,
                } => Node::Split {
                    split: *split,
                    ratio: *ratio,
                    first: Box::new(walk(first, map)),
                    second: Box::new(walk(second, map)),
                },
            }
        }
        Self {
            root: walk(&self.root, &mut map),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 | (2 / 3): pane 1 on the left, 2 above 3 on the right.
    fn three() -> Layout {
        let mut layout = Layout::single(1);
        assert!(layout.split(1, Axis::Horizontal, 2, true));
        assert!(layout.split(2, Axis::Vertical, 3, true));
        layout
    }

    fn rect_of(layout: &Layout, pane: PaneId) -> Rect {
        layout
            .rects()
            .into_iter()
            .find(|r| r.pane == pane)
            .unwrap()
            .rect
    }

    #[test]
    fn splitting_builds_the_tree_in_reading_order() {
        let layout = three();
        assert_eq!(layout.panes(), [1, 2, 3]);
        assert_eq!(
            rect_of(&layout, 1),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 1.0
            }
        );
        assert_eq!(
            rect_of(&layout, 3),
            Rect {
                x: 0.5,
                y: 0.5,
                width: 0.5,
                height: 0.5
            }
        );
        let mut before = Layout::single(1);
        assert!(before.split(1, Axis::Vertical, 9, false));
        assert_eq!(before.panes(), [9, 1], "before: on top");
        assert!(
            !before.split(42, Axis::Vertical, 10, true),
            "unknown target"
        );
        assert!(!before.split(1, Axis::Vertical, 9, true), "duplicate id");
    }

    #[test]
    fn closing_gives_the_space_to_the_sibling() {
        let mut layout = three();
        assert_eq!(layout.close(2), Some(3));
        assert_eq!(layout.panes(), [1, 3]);
        assert_eq!(rect_of(&layout, 3).height, 1.0);
        assert_eq!(layout.close(1), Some(3));
        assert_eq!(layout.panes(), [3]);
        assert_eq!(layout.close(3), None, "the last pane stays");
        assert_eq!(layout.close(7), None);
    }

    #[test]
    fn neighbors_follow_the_geometry() {
        let layout = three();
        assert_eq!(layout.neighbor(1, Direction::Right), Some(2));
        assert_eq!(layout.neighbor(2, Direction::Down), Some(3));
        assert_eq!(layout.neighbor(3, Direction::Up), Some(2));
        assert_eq!(layout.neighbor(3, Direction::Left), Some(1));
        assert_eq!(layout.neighbor(1, Direction::Left), None);
        assert_eq!(layout.neighbor(1, Direction::Up), None);

        // With a larger bottom pane, Right from 1 prefers the one it overlaps most.
        let mut uneven = three();
        assert!(uneven.set_ratio(&[true], 0.2));
        assert_eq!(uneven.neighbor(1, Direction::Right), Some(3));
    }

    #[test]
    fn resizing_moves_the_nearest_divider_on_that_side() {
        let mut layout = three();
        assert!(layout.resize(1, Direction::Right, 0.1));
        assert!((rect_of(&layout, 1).width - 0.6).abs() < 1e-9);
        assert!(
            layout.resize(2, Direction::Left, 0.1),
            "2 grows to the left"
        );
        assert!((rect_of(&layout, 1).width - 0.5).abs() < 1e-9);
        // 3 grows upward inside the right column.
        assert!(layout.resize(3, Direction::Up, 0.1));
        assert!((rect_of(&layout, 3).height - 0.6).abs() < 1e-9);
        assert!(
            !layout.resize(1, Direction::Left, 0.1),
            "no divider on that side"
        );
        for _ in 0..40 {
            layout.resize(1, Direction::Right, 0.1);
        }
        assert!(rect_of(&layout, 1).width <= MAX_RATIO + 1e-9, "clamped");
    }

    #[test]
    fn dividers_ratios_swap_and_equalize() {
        let mut layout = three();
        let dividers = layout.dividers();
        assert_eq!(dividers.len(), 2);
        assert_eq!(dividers[0].path, Vec::<bool>::new());
        assert_eq!(dividers[1].path, vec![true]);
        assert_eq!(dividers[1].axis, Axis::Vertical);
        assert!(layout.set_ratio(&[true], 2.0));
        assert!((layout.dividers()[1].ratio - MAX_RATIO).abs() < 1e-9);
        assert!(!layout.set_ratio(&[false], 0.3), "a pane, not a split");
        assert!(layout.swap(1, 3));
        assert_eq!(layout.panes(), [3, 2, 1]);
        assert!(!layout.swap(1, 1));
        layout.equalize();
        assert!(
            layout
                .dividers()
                .iter()
                .all(|d| (d.ratio - 0.5).abs() < 1e-9)
        );
    }

    #[test]
    fn serialization_round_trips_and_rejects_duplicates() {
        let layout = three();
        let json = serde_json::to_string(&layout).unwrap();
        assert!(json.contains("\"split\":\"horizontal\""), "{json}");
        let back: Layout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, layout);
        let node: Node = serde_json::from_str(
            r#"{"split":"vertical","ratio":7,"first":{"pane":1},"second":{"pane":1}}"#,
        )
        .unwrap();
        assert!(Layout::from_node(node).is_err());
        let node: Node = serde_json::from_str(
            r#"{"split":"vertical","ratio":7,"first":{"pane":1},"second":{"pane":2}}"#,
        )
        .unwrap();
        let fixed = Layout::from_node(node).unwrap();
        assert!((fixed.dividers()[0].ratio - MAX_RATIO).abs() < 1e-9);
        assert_eq!(fixed.map_panes(|id| id * 10).panes(), [10, 20]);
    }
}
