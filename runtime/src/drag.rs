use crate::id::Id;

#[derive(Clone, Copy, Debug)]
pub enum DragPhase {
    Start,
    Move,
    End,
}

impl DragPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            DragPhase::Start => "start",
            DragPhase::Move => "move",
            DragPhase::End => "end",
        }
    }
}
#[derive(Clone, Debug)]
pub struct DragEvent {
    pub phase: DragPhase,
    /// The pointer in the dragged element's own units, zoom undone — the same point a click or
    /// hover reports. `delta` is this minus where the press landed.
    pub at: (f32, f32),
    pub pos: (f32, f32), // current pos with origin as grabbed elements origin as (0,0)
    pub delta: (f32, f32), //pos-start
    pub grab: (f32, f32), //offset from origin rect
    pub scale: f32,
    pub mods: Mods,
    /// The shape the press landed on, and the pointer in *its* coordinates. Held for the whole
    /// gesture: a drag reports what it grabbed, not whatever has slid under the pointer since,
    /// and it keeps reporting once the pointer leaves the shape — which is what grabbing means.
    pub shape: Option<(Id, (f32, f32))>,
    /// Monotonic seconds since the app started, taken when the pointer event arrived rather than
    /// when the frame it lands in is drawn — several moves often arrive within one frame, and
    /// frame time would give them all the same stamp and a velocity of `dx / 0`. Same epoch as
    /// `FrameTick::elapsed`, so a release can be measured against the frames that follow it.
    pub t: f64,
}

#[derive(Clone, Debug)]
pub struct DropEvent {
    pub pos: (f32, f32), // current pos with target rect as origin
    pub mods: Mods,
    pub dragged: Id,
    pub phase: DropPhase,
    pub size: (f32, f32),
}
#[derive(Clone, Copy, Debug)]
pub enum DropPhase {
    Over,
    Release,
}

impl DropPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            DropPhase::Over => "over",
            DropPhase::Release => "release",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}
