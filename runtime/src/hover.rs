//! Pointer-over events. An element is over while the pointer is inside it — the test `hover_fill`
//! paints with — so a parent stays over its children, and one element on top doesn't hide another.

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

#[derive(Clone, Copy, Debug)]
pub struct HoverEvent {
    pub phase: HoverPhase,
    /// Element-local like a click; outside the element on leave.
    pub pos: (f32, f32),
}

/// The hover elements the pointer was inside at the last sample.
#[derive(Default)]
pub(crate) struct Hovered(Vec<Id>);

impl Hovered {
    /// Diffs one pointer sample against this frame's hover regions: the phase each fires, if any.
    /// An element gone from the view is forgotten without a leave; its handler went with it.
    pub fn step<'a>(
        &mut self,
        regions: impl IntoIterator<Item = (&'a Id, bool)>,
    ) -> Vec<Option<HoverPhase>> {
        let mut now = Vec::new();
        let mut phases = Vec::new();
        for (id, inside) in regions {
            let was = self.0.contains(id);
            if inside {
                now.push(id.clone());
            }
            phases.push(match (was, inside) {
                (false, true) => Some(HoverPhase::Enter),
                (true, true) => Some(HoverPhase::Move),
                (true, false) => Some(HoverPhase::Leave),
                (false, false) => None,
            });
        }
        self.0 = now;
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
        assert_eq!(h.step([(&card, true), (&chip, false)]), [Some(Enter), None]);
        // Nested: moving onto the chip keeps the card over.
        assert_eq!(
            h.step([(&card, true), (&chip, true)]),
            [Some(Move), Some(Enter)]
        );
        assert_eq!(
            h.step([(&card, false), (&chip, false)]),
            [Some(Leave), Some(Leave)]
        );
        assert_eq!(h.step([(&card, false), (&chip, false)]), [None, None]);
    }

    #[test]
    fn a_vanished_element_enters_again_when_it_returns() {
        let card = Id::from("card");
        let mut h = Hovered::default();
        h.step([(&card, true)]);
        assert_eq!(h.step([]), []);
        assert_eq!(h.step([(&card, true)]), [Some(Enter)]);
    }
}
