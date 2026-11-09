//! Input → present latency of UI transitions: one log line per transition, tagged by what changed and by the UI path.

use std::cell::RefCell;
use std::time::Duration;
use web_time::Instant;

// An input older than this did not cause the transition (focus loss, a poll-driven open).
const STALE: Duration = Duration::from_secs(2);

struct Pending {
    tag: String,
    ui: &'static str,
    start: Instant,
    origin: &'static str,
}

#[derive(Default)]
struct Tracker {
    input: Option<Instant>,
    pending: Option<Pending>,
    menu_opened: bool,
}

impl Tracker {
    // A later `begin` before the present retags the same transition and keeps its start.
    fn begin(&mut self, tag: String, ui: &'static str, now: Instant) {
        let input = self
            .input
            .take()
            .filter(|t| now.duration_since(*t) <= STALE);
        let (start, origin) = match (input, &self.pending) {
            (Some(t), _) => (t, "input"),
            (None, Some(p)) => (p.start, p.origin),
            (None, None) => (now, "no-input"),
        };
        self.pending = Some(Pending {
            tag,
            ui,
            start,
            origin,
        });
    }

    fn begin_menu_open(&mut self, ui: &'static str, now: Instant) {
        let kind = if std::mem::replace(&mut self.menu_opened, true) {
            "warm"
        } else {
            "cold"
        };
        self.begin(format!("menu-open {kind}"), ui, now);
    }

    fn presented(&mut self, now: Instant) -> Option<String> {
        let p = self.pending.take()?;
        Some(format!(
            "input→present: {} ui={} {:.1} ms origin={}",
            p.tag,
            p.ui,
            now.duration_since(p.start).as_secs_f64() * 1000.0,
            p.origin,
        ))
    }
}

thread_local!(static TRACKER: RefCell<Tracker> = RefCell::default());

// A discrete user action happened now.
pub fn input() {
    TRACKER.with_borrow_mut(|t| t.input = Some(Instant::now()));
}

pub fn begin(tag: String, ui: &'static str) {
    TRACKER.with_borrow_mut(|t| t.begin(tag, ui, Instant::now()));
}

// The port menu opens: cold is the first open of this process.
pub fn begin_menu_open(ui: &'static str) {
    TRACKER.with_borrow_mut(|t| t.begin_menu_open(ui, Instant::now()));
}

// A frame reached the screen; ends the pending transition, if any.
pub fn presented() {
    if let Some(line) = TRACKER.with_borrow_mut(|t| t.presented(Instant::now())) {
        log::info!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn counts_from_the_input_to_the_first_present_only() {
        let (mut t, t0) = (Tracker::default(), Instant::now());
        t.input = Some(t0);
        t.begin("card>confirm".into(), "slint", t0 + ms(5));
        let line = t.presented(t0 + ms(30)).unwrap();
        assert_eq!(
            line,
            "input→present: card>confirm ui=slint 30.0 ms origin=input"
        );
        assert_eq!(t.presented(t0 + ms(46)), None);
    }

    #[test]
    fn first_menu_open_is_cold() {
        let (mut t, t0) = (Tracker::default(), Instant::now());
        t.begin_menu_open("egui", t0);
        assert!(t.presented(t0).unwrap().contains("menu-open cold"));
        t.begin_menu_open("egui", t0);
        assert!(t.presented(t0).unwrap().contains("menu-open warm"));
    }

    #[test]
    fn retag_keeps_the_start_and_a_stale_input_is_ignored() {
        let (mut t, t0) = (Tracker::default(), Instant::now());
        t.input = Some(t0);
        t.begin("menu-open cold".into(), "egui", t0 + ms(1));
        t.begin("background-resume to menu".into(), "egui", t0 + ms(2));
        let line = t.presented(t0 + ms(10)).unwrap();
        assert!(line.contains("background-resume to menu ui=egui 10.0 ms origin=input"));
        t.input = Some(t0);
        t.begin("menu-resume".into(), "egui", t0 + STALE + ms(1));
        let line = t.presented(t0 + STALE + ms(4)).unwrap();
        assert!(line.contains("3.0 ms origin=no-input"));
    }
}
