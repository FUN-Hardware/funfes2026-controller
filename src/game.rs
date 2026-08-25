use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel, watch};

use crate::types::{CalibKind, CalibStatus, GameEvent};

const CORNER_COUNT: usize = 4;

pub struct TriggerRouter<'a> {
    trigger_receiver: channel::Receiver<'a, CriticalSectionRawMutex, (), 3>,
    calib_receiver: watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    calib_sender: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    gyro_receiver: watch::Receiver<'a, CriticalSectionRawMutex, (f32, f32), 3>,
    game_event_sender: channel::Sender<'a, CriticalSectionRawMutex, GameEvent, 3>,
    corners: [(f32, f32); CORNER_COUNT],
    corner_count: usize,
}

impl<'a> TriggerRouter<'a> {
    pub fn new(
        trigger_receiver: channel::Receiver<'a, CriticalSectionRawMutex, (), 3>,
        calib_receiver: watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, 3>,
        calib_sender: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
        gyro_receiver: watch::Receiver<'a, CriticalSectionRawMutex, (f32, f32), 3>,
        game_event_sender: channel::Sender<'a, CriticalSectionRawMutex, GameEvent, 3>,
    ) -> Self {
        Self {
            trigger_receiver,
            calib_receiver,
            calib_sender,
            gyro_receiver,
            game_event_sender,
            corners: [(0.0, 0.0); CORNER_COUNT],
            corner_count: 0,
        }
    }

    async fn handle_trigger(&mut self) {
        match self.calib_receiver.try_get() {
            Some(CalibStatus::Running(CalibKind::Orientation)) => self.record_corner(),
            Some(CalibStatus::Idle) | None => {
                self.game_event_sender.send(GameEvent::Fired).await;
            }
            Some(CalibStatus::Selecting) | Some(CalibStatus::Running(CalibKind::Stationary)) => {}
        }
    }

    fn record_corner(&mut self) {
        let Some(angles) = self.gyro_receiver.try_get() else {
            return;
        };

        self.corners[self.corner_count] = angles;
        self.corner_count += 1;

        if self.corner_count == CORNER_COUNT {
            self.corner_count = 0;
            self.calib_sender.send(CalibStatus::Idle);
        }
    }
}

#[embassy_executor::task]
pub async fn trigger_router_task(mut router: TriggerRouter<'static>) {
    loop {
        router.trigger_receiver.receive().await;
        router.handle_trigger().await;
    }
}

#[embassy_executor::task]
pub async fn game_state_task() {
    todo!()
}
