use std::cell::RefCell;

// Sound effect ids (13 §1.1).
pub const PLST: i32 = 0;
pub const ENEP0: i32 = 2;
pub const ENEP1: i32 = 3;
pub const PLDEAD: i32 = 4;
pub const TAN00: i32 = 7;
pub const OK: i32 = 10;
pub const CANCEL: i32 = 11;
pub const SELECT: i32 = 12;
pub const CAT: i32 = 14;
pub const TAN00_LOUD: i32 = 15;
pub const DAMAGE0: i32 = 20;
pub const ITEM0: i32 = 21;
pub const KIRA0: i32 = 25;
pub const EXTEND: i32 = 28;
pub const TIMEOUT: i32 = 29;
pub const GRAZE: i32 = 30;
pub const POWERUP: i32 = 31;
pub const PAUSE: i32 = 34;
pub const CARDGET: i32 = 35;
pub const OPTION: i32 = 36;
pub const DAMAGE1: i32 = 37;
pub const TIMEOUT2: i32 = 38;
pub const OPSHOW: i32 = 39;
pub const OPHIDE: i32 = 40;
pub const INVALID: i32 = 41;
pub const ITEM1: i32 = 44;
pub const OK_CLOCK: i32 = 45;

// SE table (13 §1.1): wave file, volume in 1/100 dB.
pub const TABLE: [(&str, i16); 46] = [
    ("se_plst00", -1900),
    ("se_plst00", -2100),
    ("se_enep00", -1200),
    ("se_enep00", -1500),
    ("se_pldead00", -1100),
    ("se_power0", -700),
    ("se_power1", -700),
    ("se_tan00", -1900),
    ("se_tan01", -2200),
    ("se_tan02", -2400),
    ("se_ok00", -1100),
    ("se_cancel00", -1100),
    ("se_select00", -1500),
    ("se_gun00", -1500),
    ("se_cat00", -1000),
    ("se_tan00", -1100),
    ("se_lazer00", -1300),
    ("se_lazer01", -1400),
    ("se_enep01", -900),
    ("se_nep00", -400),
    ("se_damage00", -880),
    ("se_item00", -1500),
    ("se_tan00", -300),
    ("se_tan01", -1800),
    ("se_tan02", -1800),
    ("se_kira00", -1100),
    ("se_kira01", -1300),
    ("se_kira02", -1500),
    ("se_extend", -500),
    ("se_timeout", -500),
    ("se_graze", -1100),
    ("se_powerup", -800),
    ("se_graze", -1200),
    ("se_kira00", -500),
    ("se_pause", -800),
    ("se_cardget", -800),
    ("se_option", -800),
    ("se_damage01", -700),
    ("se_timeout2", -300),
    ("se_opshow", -800),
    ("se_ophide", -800),
    ("se_invalid", -200),
    ("se_slash", 0),
    ("se_slash", -600),
    ("se_item01", -800),
    ("se_ok00", -100),
];

const MAX_IDS: usize = 12;
const MAX_PANS: usize = 128;

// Per-frame queue (13 §1.2): up to 12 distinct ids, each with up to 128 pan values.
#[derive(Default, Debug)]
pub struct Queue {
    slots: Vec<(i32, Vec<i32>)>,
}

impl Queue {
    pub fn push(&mut self, id: i32, pan: i32) {
        if !(0..TABLE.len() as i32).contains(&id) {
            return;
        }
        if let Some((_, pans)) = self.slots.iter_mut().find(|(i, _)| *i == id) {
            if pans.len() < MAX_PANS {
                pans.push(pan);
            }
        } else if self.slots.len() < MAX_IDS {
            self.slots.push((id, vec![pan]));
        }
    }

    // Sound_Update: (id, integer average pan) in queue order; the queue is cleared.
    pub fn drain(&mut self) -> Vec<(i32, i32)> {
        self.slots
            .drain(..)
            .map(|(id, p)| (id, p.iter().sum::<i32>() / p.len() as i32))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

thread_local! {
    static QUEUE: RefCell<Queue> = RefCell::default();
}

// Sound_Play(id, pan).
pub fn play(id: i32) {
    QUEUE.with_borrow_mut(|q| q.push(id, 0));
}

// Sound_PlayAt(id, x): pan from the playfield x.
pub fn play_at(id: i32, x: f32) {
    QUEUE.with_borrow_mut(|q| q.push(id, ((x - 192.0) * 1000.0 / 192.0) as i32));
}

pub fn take() -> Vec<(i32, i32)> {
    QUEUE.with_borrow_mut(|q| q.drain())
}

pub fn pending() -> bool {
    QUEUE.with_borrow(|q| !q.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_dedup_and_pan() {
        let mut q = Queue::default();
        q.push(30, 100);
        q.push(30, -300);
        q.push(2, 0);
        for id in 3..20 {
            q.push(id, 0);
        }
        q.push(30, 0);
        q.push(99, 0);
        let d = q.drain();
        assert_eq!(d.len(), 12);
        assert_eq!(d[0], (30, (100 - 300) / 3));
        assert_eq!(d[1], (2, 0));
        assert_eq!(d[11].0, 12);
        assert!(q.is_empty());
        for _ in 0..200 {
            q.push(5, 1000);
        }
        q.push(5, -1000);
        assert_eq!(q.drain(), vec![(5, 1000)]);
    }

    #[test]
    fn pan_from_x() {
        play_at(4, 384.0);
        play_at(5, 0.0);
        play_at(6, 192.0);
        assert_eq!(take(), vec![(4, 1000), (5, -1000), (6, 0)]);
    }
}
