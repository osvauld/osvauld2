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
#[derive(Clone, Copy, Debug)]
pub struct DragEvent {
    pub phase: DragPhase,
    pub pos: (f32, f32), // current pos with origin as grabbed elements origin as (0,0)
    pub delta: (f32, f32), //pos-start
    pub grab: (f32, f32), //offset from origin rect
    pub mods: Mods,
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
#[derive(Clone, Copy, Debug)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}
