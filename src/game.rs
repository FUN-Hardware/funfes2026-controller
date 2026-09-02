use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel, watch};

use crate::types::{CalibKind, CalibStatus, SoundEvent};

const CORNER_COUNT: usize = 4;

pub struct TriggerRouter<'a> {
    trigger_receiver: channel::Receiver<'a, CriticalSectionRawMutex, (), 3>,
    calib_receiver: watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    calib_sender: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    gyro_receiver: watch::Receiver<'a, CriticalSectionRawMutex, (f32, f32), 3>,
    ammo_sender: watch::Sender<'a, CriticalSectionRawMutex, u8, 3>,
    sound_event_sender: channel::Sender<'a, CriticalSectionRawMutex, SoundEvent, 3>,
    orientation_range_sender: watch::Sender<'a, CriticalSectionRawMutex, [(f32, f32); 2], 1>,
    corners: [(f32, f32); CORNER_COUNT],
    corner_count: usize,
}

impl<'a> TriggerRouter<'a> {
    pub fn new(
        trigger_receiver: channel::Receiver<'a, CriticalSectionRawMutex, (), 3>,
        calib_receiver: watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, 3>,
        calib_sender: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
        gyro_receiver: watch::Receiver<'a, CriticalSectionRawMutex, (f32, f32), 3>,
        ammo_sender: watch::Sender<'a, CriticalSectionRawMutex, u8, 3>,
        sound_event_sender: channel::Sender<'a, CriticalSectionRawMutex, SoundEvent, 3>,
        orientation_range_sender: watch::Sender<'a, CriticalSectionRawMutex, [(f32, f32); 2], 1>,
    ) -> Self {
        Self {
            trigger_receiver,
            calib_receiver,
            calib_sender,
            gyro_receiver,
            ammo_sender,
            sound_event_sender,
            orientation_range_sender,
            corners: [(0.0, 0.0); CORNER_COUNT],
            corner_count: 0,
        }
    }

    async fn handle_trigger(&mut self) {
        let mut fire_flag = false;
        match self.calib_receiver.try_get() {
            Some(CalibStatus::Running(CalibKind::Orientation)) => self.record_corner(),
            Some(CalibStatus::Idle) | None => {
                if let Some(ammo) = self.ammo_sender.try_get()
                    && ammo > 0
                {
                    fire_flag = true;
                    self.ammo_sender.send(ammo - 1);
                } else {
                    self.ammo_sender.send(0);
                }
            }
            Some(CalibStatus::Selecting) | Some(CalibStatus::Running(CalibKind::Stationary)) => {}
        }
        if fire_flag {
            self.sound_event_sender.send(SoundEvent::Fire).await;
        }
    }

    fn record_corner(&mut self) {
        let Some(angles) = self.gyro_receiver.try_get() else {
            return;
        };

        self.corners[self.corner_count] = angles;
        self.corner_count += 1;

        let mut pitch = [0.0; 4];
        let mut yaw = [0.0; 4];

        for (i, &(p, y)) in self.corners.iter().take(self.corner_count).enumerate() {
            pitch[i] = p;
            yaw[i] = y;
        }

        if self.corner_count == CORNER_COUNT {
            self.corner_count = 0;
            pitch.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            yaw.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

            let pitch = ((pitch[0] + pitch[1]) / 2.0, (pitch[2] + pitch[3]) / 2.0);
            let yaw = ((yaw[0] + yaw[1]) / 2.0, (yaw[2] + yaw[3]) / 2.0);

            crate::debug_println!("pitch: {:?} yaw: {:?}", pitch, yaw);

            self.orientation_range_sender.send([pitch, yaw]);
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
