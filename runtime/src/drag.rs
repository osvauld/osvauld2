#[derive(Clone, Copy, Debug)]
pub enum DragPhase {
    Start,
    Move,
    End,
}
#[derive(Clone, Copy, Debug)]
pub struct DragEvent {
    pub phase: DragPhase,
    pub pos: (f32, f32), // current pos with origin as grabbed elements origin as (0,0)
    pub delta: (f32, f32), //pos-start
    pub mods: Mods,
}
#[derive(Clone, Copy, Debug)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}
