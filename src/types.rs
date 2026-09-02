#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalibKind {
    Orientation,
    Stationary,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalibStatus {
    Idle,
    Selecting,
    Running(CalibKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundEvent {
    Fire,
    Reload,
}
