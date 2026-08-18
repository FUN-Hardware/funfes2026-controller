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
pub struct GameState {
    pub ammo: u8,
    pub calib: CalibStatus,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameEvent {
    Fired,
    Reloaded,
    Calib(CalibStatus),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundEvent {
    Fire,
    Reload,
}
