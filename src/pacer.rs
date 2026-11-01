//! Frame_Run 0x441E70 logic pacing (00 §2) without the window: 1/60 s deadline steps, late frames dropped, plus the
//! port's turbo and suspend / resume (ARCH_D §4, §5). Pure: the caller passes the time and does any waiting itself.

use std::time::Duration;
use web_time::Instant;

// Frame_Run deadline step, 1/60 s (00 §2).
pub const FRAME: f64 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tick {
    // Run this many logic frames now, then draw once.
    Run(u32),
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

    // One logic step per deadline; a frame due at or before the deadline waits (`now <= deadline`, 00 §2). Late frames
    // are dropped: the deadline jumps past `now` in whole steps and only one frame runs. The monotonic clock never
    // runs backwards, so Frame_Run's timer-wrap reset is not needed.
    pub fn poll(&mut self, now: Instant) -> Tick {
        if self.suspended {
            return Tick::Suspended;
        }
        let now = self.secs(now);
        if self.turbo == 0 && now <= self.deadline {
            return Tick::Wait(self.deadline - now);
        }
        while self.deadline < now {
            self.deadline += FRAME;
        }
        Tick::Run(self.turbo.max(1))
    }

    pub fn turbo(&self) -> u32 {
        self.turbo
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
            Tick::Run(n) => n,
            _ => 0,
        }
    }

    // One frame per 1/60 s; polls before the deadline wait.
    #[test]
    fn steps() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        assert_eq!(p.poll(at(&p, 0.001)), Tick::Run(1));
        assert!(
            matches!(p.poll(at(&p, 0.010)), Tick::Wait(w) if (w - (FRAME - 0.010)).abs() < 1e-9)
        );
        assert_eq!(p.poll(at(&p, 0.017)), Tick::Run(1));
        assert!(matches!(p.poll(at(&p, 0.018)), Tick::Wait(_)));
        let total: u32 = (1..=600)
            .map(|i| runs(p.poll(at(&p, 0.017 + i as f64 * 0.001))))
            .sum();
        assert_eq!(total, 36);
    }

    // A late poll runs one frame and skips the missed deadlines (drop-late, not catch-up).
    #[test]
    fn late_frames_dropped() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        assert_eq!(p.poll(at(&p, 0.001)), Tick::Run(1));
        assert_eq!(p.poll(at(&p, 0.505)), Tick::Run(1));
        assert!(matches!(p.poll(at(&p, 0.506)), Tick::Wait(_)));
        let d = (p.next_deadline().unwrap() - p.epoch).as_secs_f64();
        assert!(d > 0.505 && d <= 0.505 + FRAME + 1e-9);
    }

    // Turbo never waits and runs n frames per poll.
    #[test]
    fn turbo() {
        let mut p = Pacer::with_epoch(Instant::now(), 8);
        assert_eq!(p.poll(at(&p, 0.001)), Tick::Run(8));
        assert_eq!(p.poll(at(&p, 0.001)), Tick::Run(8));
        assert!(p.next_deadline().unwrap() <= at(&p, 0.001 + FRAME));
    }

    // Suspended: no frames, no deadline; resume restarts the deadline at now, so the next poll runs one frame.
    #[test]
    fn suspend_resume() {
        let mut p = Pacer::with_epoch(Instant::now(), 0);
        assert_eq!(p.poll(at(&p, 0.001)), Tick::Run(1));
        p.suspend();
        assert_eq!(p.poll(at(&p, 1.0)), Tick::Suspended);
        assert_eq!(p.next_deadline(), None);
        p.resume(at(&p, 10.0));
        assert_eq!(p.next_deadline(), Some(at(&p, 10.0)));
        assert!(matches!(p.poll(at(&p, 10.0)), Tick::Wait(_)));
        assert_eq!(p.poll(at(&p, 10.001)), Tick::Run(1));
        assert!(matches!(p.poll(at(&p, 10.002)), Tick::Wait(_)));
    }
}
