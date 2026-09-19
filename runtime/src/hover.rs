//! Pointer-over events. An element is over while the pointer is inside it — the test `hover_fill`
//! paints with — so a parent stays over its children, and one element on top doesn't hide another.

use crate::frame::FrameHit;
use crate::id::Id;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HoverPhase {
    Enter,
    Move,
    Leave,
}

impl HoverPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            HoverPhase::Enter => "enter",
            HoverPhase::Move => "move",
            HoverPhase::Leave => "leave",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HoverEvent {
    pub phase: HoverPhase,
    /// Element-local like a click; outside the element on leave.
    pub pos: (f32, f32),
    /// The named shape under the pointer, when the element draws a Frame that has any.
    pub shape: Option<FrameHit>,
}

/// The hover elements the pointer was inside at the last sample, each with the shape it was on,
/// and where the pointer was when that was taken.
#[derive(Default)]
pub(crate) struct Hovered {
    inside: Vec<(Id, Option<FrameHit>)>,
    last: Option<(f32, f32)>,
}

impl Hovered {
    /// Diffs one pointer sample against this frame's hover regions: the phase each fires, if any.
    /// An element gone from the view is forgotten without a leave; its handler went with it.
    ///
    /// This is sampled once per frame as well as once per pointer event, so `Move` needs something
    /// to have actually changed — otherwise a still pointer would report one every frame, and the
    /// redraw each dispatch requests would never settle.
    ///
    /// The whole `FrameHit` is what's compared, not just the shape's id: its `local` point is the
    /// pointer's offset inside the shape, which drifts as the shape moves and is the other half of
    /// what an app is told. Comparing it costs nothing when nothing moves — the same geometry and
    /// the same pointer recompute the same point — so this stays quiet in a still app by itself.
    pub fn step<'a>(
        &mut self,
        at: (f32, f32),
        regions: impl IntoIterator<Item = (&'a Id, bool, Option<&'a FrameHit>)>,
    ) -> Vec<Option<HoverPhase>> {
        let moved = self.last != Some(at);
        let mut now = Vec::new();
        let mut phases = Vec::new();
        for (id, inside, shape) in regions {
            let was = self.inside.iter().find(|(i, _)| i == id).map(|(_, s)| s);
            if inside {
                now.push((id.clone(), shape.cloned()));
            }
            phases.push(match (was, inside) {
                (None, true) => Some(HoverPhase::Enter),
                (Some(prev), true) if moved || prev.as_ref() != shape => Some(HoverPhase::Move),
                (Some(_), true) => None,
                (Some(_), false) => Some(HoverPhase::Leave),
                (None, false) => None,
            });
        }
        self.inside = now;
        self.last = Some(at);
        phases
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HoverPhase::*;

    #[test]
    fn enter_move_leave_per_element() {
        let (card, chip) = (Id::from("card"), Id::from("chip"));
        let mut h = Hovered::default();
        assert_eq!(
            h.step((0.0, 0.0), [(&card, true, None), (&chip, false, None)]),
            [Some(Enter), None]
        );
        // Nested: moving onto the chip keeps the card over.
        assert_eq!(
            h.step((1.0, 0.0), [(&card, true, None), (&chip, true, None)]),
            [Some(Move), Some(Enter)]
        );
        assert_eq!(
            h.step((9.0, 9.0), [(&card, false, None), (&chip, false, None)]),
            [Some(Leave), Some(Leave)]
        );
        assert_eq!(
            h.step((9.0, 9.0), [(&card, false, None), (&chip, false, None)]),
            [None, None]
        );
    }

    #[test]
    fn a_vanished_element_enters_again_when_it_returns() {
        let card = Id::from("card");
        let mut h = Hovered::default();
        h.step((0.0, 0.0), [(&card, true, None)]);
        assert_eq!(h.step((0.0, 0.0), []), []);
        assert_eq!(h.step((0.0, 0.0), [(&card, true, None)]), [Some(Enter)]);
    }

    fn hit(id: &str, x: f64) -> FrameHit {
        FrameHit {
            id: Id::from(id),
            local: vello::kurbo::Point::new(x, 0.0),
            into: vello::kurbo::Affine::IDENTITY,
        }
    }

    /// The case an animating app is for: the pointer is parked and the diagram moves under it.
    /// Both halves of the answer count as a change — which shape, and where inside it.
    #[test]
    fn a_still_pointer_moves_when_the_geometry_under_it_does() {
        let card = Id::from("card");
        let (third, fourth) = (hit("cell:3", 8.0), hit("cell:4", 8.0));
        let at = (5.0, 5.0);
        let mut h = Hovered::default();
        assert_eq!(h.step(at, [(&card, true, Some(&third))]), [Some(Enter)]);
        assert_eq!(h.step(at, [(&card, true, Some(&third))]), [None]);
        assert_eq!(h.step(at, [(&card, true, Some(&fourth))]), [Some(Move)]);
        // Same shape, slid 2pt along under the pointer: the offset it reports is now wrong, so
        // saying nothing here would leave the app holding a stale one until the shape changed.
        assert_eq!(
            h.step(at, [(&card, true, Some(&hit("cell:4", 6.0)))]),
            [Some(Move)]
        );
        // Drifting off every shape while staying inside the element is a change too.
        assert_eq!(h.step(at, [(&card, true, None)]), [Some(Move)]);
        assert_eq!(h.step(at, [(&card, true, None)]), [None]);
    }
}
