//! Frame_Run 0x441E70 logic pacing (00 §2) without the window, plus the port's turbo, replay transport and suspend / resume.

use std::time::Duration;
use web_time::Instant;

// Frame_Run deadline step, 1/60 s.
pub const FRAME: f64 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tick {
    // `draws` times: `per_draw` logic frames, then one draw; one Present at the end.
    Run { per_draw: u32, draws: u32 },
    // Replay transport paused: no logic frames, no draw; the last frame is presented again.
    Held,
    // Not yet due: seconds until the deadline.
    Wait(f64),
    // Suspended: no logic frames, no draw.
    Suspended,
}

#[derive(Clone, Debug)]
pub struct Pacer {
    epoch: Instant,
    // Next-frame deadline, seconds since `epoch`.
    deadline: f64,
    // Debug `TH08_TURBO=<n>`: n logic frames per drawn frame, no pacing; 0 = off.
    turbo: u32,
    suspended: bool,
    // Replay transport: `rate.0` logic frames every `rate.1` deadlines; held = paused, `step` = one frame owed.
    rate: (u32, u32),
    held: bool,
    step: bool,
}

impl Pacer {
    pub fn new(turbo: u32) -> Self {
        Self::with_epoch(Instant::now(), turbo)
    }

    pub fn with_epoch(epoch: Instant, turbo: u32) -> Self {
        Self {
            epoch,
            deadline: 0.0,
            turbo,
            suspended: false,
            rate: (1, 1),
            held: false,
            step: false,
        }
    }

    // The game is loaded: the clock starts now, the first poll runs a frame.
    pub fn start(&mut self, now: Instant) {
        self.epoch = now;
        self.deadline = 0.0;
    }

    fn secs(&self, now: Instant) -> f64 {
        now.saturating_duration_since(self.epoch).as_secs_f64()
    }

    // One logic step per deadline; a frame due at or before the deadline waits (`now <= deadline`). Late frames
    // are dropped: the deadline jumps past `now` in whole steps and only one frame runs. The monotonic clock never
    // runs backwards, so Frame_Run's timer-wrap reset is not needed.
    pub fn poll(&mut self, now: Instant) -> Tick {
        if self.suspended {
            return Tick::Suspended;
        }
        let now = self.secs(now);
        if (self.turbo == 0 || self.held) && now <= self.deadline {
            return Tick::Wait(self.deadline - now);
        }
        let step = if self.held {
            FRAME
        } else {
            FRAME * f64::from(self.rate.1)
        };
        while self.deadline < now {
            self.deadline += step;
        }
        if self.held {
            return if std::mem::take(&mut self.step) {
                Tick::Run {
                    per_draw: 1,
                    draws: 1,
                }
            } else {
                Tick::Held
            };
        }
        Tick::Run {
            per_draw: self.turbo.max(1),
            draws: self.rate.0,
        }
    }

    pub fn turbo(&self) -> u32 {
        self.turbo
    }

    // 1× and running: the original pacing (and the real clock).
    pub fn is_original(&self) -> bool {
        self.rate == (1, 1) && !self.held
    }

    // `num` logic frames per `den` frame periods (one of them is 1); the deadline restarts at `now`.
    pub fn set_rate(&mut self, rate: (u32, u32), now: Instant) {
        self.rate = rate;
        self.deadline = self.secs(now);
    }

    pub fn rate(&self) -> (u32, u32) {
        self.rate
    }

    pub fn hold(&mut self, on: bool, now: Instant) {
        self.held = on;
        self.step = false;
        self.deadline = self.secs(now);
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    // While held: the next due poll runs one logic frame and one draw.
    pub fn step(&mut self) {
        self.step = self.held;
    }

    pub fn suspend(&mut self) {
        self.suspended = true;
    }

    // Back from the background: the deadline restarts at `now`, so the time away is not caught up.
    pub fn resume(&mut self, now: Instant) {
        self.suspended = false;
        self.deadline = self.secs(now);
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    // For `ControlFlow::WaitUntil`: when the next frame is due; None while suspended. Turbo is always due.
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.suspended {
            return None;
        }
        Some(self.epoch + Duration::from_secs_f64(self.deadline.max(0.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(p: &Pacer, s: f64) -> Instant {
        p.epoch + Duration::from_secs_f64(s)
    }

    fn runs(t: Tick) -> u32 {
        match t {
            Tick::Run { per_draw, draws } => per_draw * draws,
            _ => 0,
        }
    }

    const ONE: Tick = Tick::Run {
        per_draw: 1,
        draws: 1,
    };

    // One frame per 1/60 s; polls before the deadline wait.
    #[test]
    fn steps() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        assert_eq!(p.poll(at(&p, 0.001)), ONE);
        assert!(
            matches!(p.poll(at(&p, 0.010)), Tick::Wait(w) if (w - (FRAME - 0.010)).abs() < 1e-9)
        );
        assert_eq!(p.poll(at(&p, 0.017)), ONE);
        assert!(matches!(p.poll(at(&p, 0.018)), Tick::Wait(_)));
        let total: u32 = (1..=600)
            .map(|i| runs(p.poll(at(&p, 0.017 + f64::from(i) * 0.001))))
            .sum();
        assert_eq!(total, 36);
    }

    // A late poll runs one frame and skips the missed deadlines (drop-late, not catch-up).
    #[test]
    fn late_frames_dropped() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        assert_eq!(p.poll(at(&p, 0.001)), ONE);
        assert_eq!(p.poll(at(&p, 0.505)), ONE);
        assert!(matches!(p.poll(at(&p, 0.506)), Tick::Wait(_)));
        let d = (p.next_deadline().unwrap() - p.epoch).as_secs_f64();
        assert!(d > 0.505 && d <= 0.505 + FRAME + 1e-9);
    }

    // Turbo never waits and runs n frames per poll.
    #[test]
    fn turbo() {
        let mut p = Pacer::with_epoch(Instant::now(), 8);
        let eight = Tick::Run {
            per_draw: 8,
            draws: 1,
        };
        assert_eq!(p.poll(at(&p, 0.001)), eight);
        assert_eq!(p.poll(at(&p, 0.001)), eight);
        assert!(p.next_deadline().unwrap() <= at(&p, 0.001 + FRAME));
    }

    // Suspended: no frames, no deadline; resume restarts the deadline at now, so the next poll runs one frame.
    #[test]
    fn suspend_resume() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        assert_eq!(p.poll(at(&p, 0.001)), ONE);
        p.suspend();
        assert_eq!(p.poll(at(&p, 1.0)), Tick::Suspended);
        assert_eq!(p.next_deadline(), None);
        p.resume(at(&p, 10.0));
        assert_eq!(p.next_deadline(), Some(at(&p, 10.0)));
        assert!(matches!(p.poll(at(&p, 10.0)), Tick::Wait(_)));
        assert_eq!(p.poll(at(&p, 10.001)), ONE);
        assert!(matches!(p.poll(at(&p, 10.002)), Tick::Wait(_)));
    }

    // 4×: four draws of one frame per deadline; ¼×: one frame every fourth deadline.
    #[test]
    fn rates() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        p.set_rate((4, 1), at(&p, 0.0));
        assert_eq!(
            p.poll(at(&p, 0.001)),
            Tick::Run {
                per_draw: 1,
                draws: 4
            }
        );
        assert!(!p.is_original());
        p.set_rate((1, 4), at(&p, 0.002));
        let total: u32 = (1..=600)
            .map(|i| runs(p.poll(at(&p, 0.002 + f64::from(i) * 0.001))))
            .sum();
        assert_eq!(total, 9);
        p.set_rate((1, 1), at(&p, 1.0));
        assert!(p.is_original());
    }

    // Held: polls re-present without logic frames; a step runs exactly one frame, then holds again.
    #[test]
    fn hold_and_step() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        p.set_rate((8, 1), at(&p, 0.0));
        p.hold(true, at(&p, 0.0));
        assert_eq!(p.poll(at(&p, 0.001)), Tick::Held);
        assert!(matches!(p.poll(at(&p, 0.002)), Tick::Wait(_)));
        p.step();
        assert_eq!(p.poll(at(&p, 0.020)), ONE);
        assert_eq!(p.poll(at(&p, 0.040)), Tick::Held);
        p.hold(false, at(&p, 0.050));
        assert_eq!(runs(p.poll(at(&p, 0.051))), 8);
        p.step();
        assert_eq!(runs(p.poll(at(&p, 0.070))), 8);
    }
}
