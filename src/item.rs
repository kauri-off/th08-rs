//! Item manager 0x1653648: spawn, per-frame calc, collection, draw (08 §8).

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::bullet::ANM_SLOT;
use crate::gfx::Gfx;
use crate::player::{Player, Stats};

// 08 §8.1: 2096 slots × 0x2E4.
pub const MAX_ITEMS: usize = 2096;
// Item types (08 §8.2); sprite script = etama.anm 0x3D + type.
pub const POWER: u8 = 0;
pub const POINT: u8 = 1;
pub const BIG_POWER: u8 = 2;
pub const BOMB: u8 = 3;
pub const FULL_POWER: u8 = 4;
pub const LIFE: u8 = 5;
pub const STAR: u8 = 6;
pub const TIME_ORB: u8 = 7;
pub const CONVERTED: u8 = 8;
// Small-popup colours per caller (13 §4 → answers_impl9 §12).
pub const POPUP_NORMAL: u32 = 0xFFFFFFFF;
pub const POPUP_MAX: u32 = 0xFFFFFF00;
pub const POPUP_POWER: u32 = 0xFFFFC0A0;
pub const POPUP_TIME: u32 = 0xDFFFFFFF;
pub const POPUP_TIME_OVER: u32 = 0xDFFFEF80;
// Tombstone in `order` for an item freed during this calc.
const DEAD: u16 = u16::MAX;
// Power level table 0x4C7A6C (08 §8.5).
const POWER_LEVELS: [f32; 6] = [8.0, 24.0, 48.0, 80.0, 128.0, 999.0];

// Item object 0x2E4 (08 §8.1).
#[derive(Clone, Debug, Default)]
pub struct Item {
    pub vm: AnmVm,
    // +0x2A4 playfield px.
    pub pos: [f32; 3],
    // +0x2B0 px/frame; in state 2 it holds the start position instead.
    pub vel: [f32; 3],
    // +0x2BC fly-to target (state 2).
    pub target: [f32; 3],
    // +0x2C8 frames since spawn.
    pub timer: Timer,
    // +0x2D4 type (08 §8.2).
    pub kind: u8,
    pub alive: bool,
    // +0x2D7: 0 fall, 1 attracted, 2 fly to target, 3/5 time-orb rise (08 §8.4).
    pub state: u8,
    // +0x2D8 max-value flag (point items collected at full value).
    pub max_flag: bool,
}

pub struct ItemManager {
    pub items: Vec<Item>,
    // +0x17ADA4 search cursor (08 §8.3).
    cursor: usize,
    // Live-item list in spawn order (08 §8.1 doubly linked list).
    order: Vec<u16>,
    // +0x17ADA8 live count, recomputed by calc.
    pub count: usize,
    // Time-orb gauge lock 0x18B89D4 = player +0xE2ADC (08 §8.4), frames.
    pub collect_timer: i32,
    // Port-only counter of collections.
    pub collected: u64,
}

impl Default for ItemManager {
    fn default() -> Self {
        Self {
            items: vec![Item::default(); MAX_ITEMS],
            cursor: 0,
            order: Vec::new(),
            count: 0,
            collect_timer: 0,
            collected: 0,
        }
    }
}

// Collection side effects that reach outside the item manager.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Collected {
    // Mode-1 cancel of all bullets (08 §8.5 full power).
    pub cancel: bool,
}

// Number of POWER_LEVELS entries ≤ power (08 §8.5).
pub fn power_level(p: f32) -> usize {
    POWER_LEVELS.iter().filter(|&&t| t <= p).count()
}

// Point item value (08 §8.5); `div` 10 gives the converted-point variant.
pub fn point_value(max: i32, y: f32, line: f32, max_flag: bool, doubled: bool, div: i32) -> i32 {
    let mut v = if y < line || max_flag {
        max
    } else {
        max / 2 - (y - line) as i32 * (max / 1000)
    };
    v /= div;
    v -= v % 10;
    if doubled {
        v *= 2;
    }
    v
}

// Next extend threshold in point items (08 §8.5): `difficulty` = gm+0x30, n = extends so far.
pub fn next_extend(difficulty: i32, n: i32) -> i32 {
    if difficulty < 4 {
        [100, 250, 500, 800, 1100, 9999]
            .get(n as usize)
            .copied()
            .unwrap_or(9999 + 500 * (n - 5))
    } else {
        [200, 666, 9999].get(n as usize).copied().unwrap_or(99999)
    }
}

impl ItemManager {
    pub fn live(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.alive)
    }

    pub fn count(&self, kind: u8) -> i32 {
        self.live().filter(|i| i.kind == kind).count() as i32
    }

    // ItemMgr_DropAttracted 0x441530 (09 §6.1): every state-1 item falls again, state 0, vel (0, −0.9, 0).
    pub fn drop_attracted(&mut self) {
        for it in self.items.iter_mut().filter(|i| i.alive && i.state == 1) {
            it.state = 0;
            it.vel = [0.0, -0.9, 0.0];
        }
    }

    // Bomb start (09 §7): every item flies to the player (state 1, velocity (0, −0.5)).
    pub fn attract_all(&mut self) {
        for it in self.items.iter_mut().filter(|i| i.alive && i.state != 1) {
            it.state = 1;
            it.vel = [0.0, -0.5, 0.0];
        }
    }

    // ItemMgr_Spawn 0x4400A0 (08 §8.3). RNG: mode 2 x then y; mode 3/5 vel.y (Float) then vel.x (Signed).
    pub fn spawn(
        &mut self,
        anm: &mut AnmManager,
        player: &Player,
        pos: [f32; 3],
        kind: i32,
        mode: u8,
    ) -> Option<usize> {
        if pos[0] < -64.0 || pos[0] > 448.0 || kind < 0 {
            return None;
        }
        let (mut kind, mut mode) = (kind as u8, mode);
        if player.stats.power >= 128.0 && (kind == POWER || kind == BIG_POWER) {
            kind = CONVERTED;
        }
        if kind == TIME_ORB {
            mode = 3;
        } else if kind == 10 {
            (kind, mode) = (TIME_ORB, 5);
        }
        // 08 §8.3: a time orb probes only the cursor slot; occupied → nothing.
        let probes = if kind == TIME_ORB { 1 } else { MAX_ITEMS };
        let mut slot = None;
        for _ in 0..probes {
            let i = self.cursor % MAX_ITEMS;
            self.cursor = i + 1;
            if !self.items[i].alive {
                slot = Some(i);
                break;
            }
        }
        let i = slot?;
        if self.cursor > MAX_ITEMS - 1 {
            self.cursor = 0;
        }
        let mut it = Item {
            pos,
            vel: [0.0, -2.2, 0.0],
            kind,
            alive: true,
            state: mode,
            ..Item::default()
        };
        match mode {
            2 => {
                let x = anm.rng.next_f32() * 288.0 + 48.0;
                let y = anm.rng.next_f32() * 192.0 - 64.0;
                it.target = [x, y, 0.0];
                it.vel = pos;
            }
            3 | 5 => {
                it.vel[1] = -2.0 - anm.rng.next_f32() * 0.2;
                it.vel[0] = 0.6 * anm.rng.next_signed();
                if player.state == 2 {
                    it.state = 0;
                    it.vel = [0.0, -0.9, 0.0];
                }
            }
            _ => {}
        }
        anm.set_script(&mut it.vm, ANM_SLOT, 0x3D + kind as usize);
        it.vm.color = [0xFF; 4];
        it.vm.flags |= anmvm::F_ZWRITE_OFF;
        self.items[i] = it;
        self.order.push(i as u16);
        Some(i)
    }

    // Power reached 128 (08 §8.5): every other type 0/2 item becomes type 8 (effect 0, script 0x45).
    fn convert_power_items(&mut self, anm: &mut AnmManager, except: usize) {
        for i in 0..MAX_ITEMS {
            let it = &mut self.items[i];
            if i == except || !it.alive || !(it.kind == POWER || it.kind == BIG_POWER) {
                continue;
            }
            it.kind = CONVERTED;
            if it.vel[1] > -0.5 {
                it.vel = [0.0, -0.5, 0.0];
            }
            let p = it.pos;
            crate::effect::spawn(anm, 0, p, 1, 0xFFFFFFFF);
            let it = &mut self.items[i];
            let mut vm = std::mem::take(&mut it.vm);
            anm.set_script(&mut vm, ANM_SLOT, 0x45);
            vm.flags |= anmvm::F_ZWRITE_OFF;
            self.items[i].vm = vm;
        }
    }

    // Collection handlers 0x440CF0–0x4412B0 (08 §8.5); score is stored /10 by `add_score`.
    fn collect(
        &mut self,
        idx: usize,
        anm: &mut AnmManager,
        p: &mut Player,
        spell: bool,
    ) -> Collected {
        let mut out = Collected::default();
        let (kind, y, max_flag) = (
            self.items[idx].kind,
            self.items[idx].pos[1],
            self.items[idx].max_flag,
        );
        let pos = self.items[idx].pos;
        let popup = |v: i32, c: u32| crate::ascii::popup(pos, v, c);
        let line = p.sht.poc_line;
        let st = &mut p.stats;
        match kind {
            POWER | BIG_POWER => {
                if st.power < 128.0 {
                    let level = power_level(st.power);
                    st.power += if kind == POWER { 1.0 } else { 8.0 };
                    // GameMgr_AddPower 0x441850 checksum (4 draws); reaching 128 adds SetPower's 4 (08 §8.5).
                    Stats::checksum(&mut anm.rng);
                    if st.power >= 128.0 {
                        st.set_power(128.0, &mut anm.rng);
                        out.cancel = !spell;
                        self.convert_power_items(anm, idx);
                    }
                    st.add_score(10);
                    if power_level(st.power) != level {
                        crate::sound::play(crate::sound::POWERUP);
                        popup(-1, POPUP_POWER);
                    } else {
                        popup(10, POPUP_NORMAL);
                    }
                }
                // 08 §8.5: type 0 always adds rank +1, even at full power.
                if kind == POWER {
                    st.rank(1);
                }
            }
            POINT => {
                let max = st.max_point;
                let v = point_value(max, y, line, max_flag, st.human_max(), 1);
                self.items[idx].max_flag = v >= max;
                popup(v, if v >= max { POPUP_MAX } else { POPUP_NORMAL });
                st.add_score(v);
                st.point_items += 1;
                st.rec.points += 1;
                st.point_items2 += 1;
                st.rank(if v >= max { 10 } else { 3 });
                while st.extends >= 0 && st.point_items2 >= st.next_extend {
                    st.extend(&mut anm.rng);
                    st.extends += 1;
                    st.next_extend = next_extend(st.difficulty, st.extends);
                }
                st.point_counter += 1;
                // Item_CollectPoint 0x440E40 updates the checksum (11 §1.5).
                Stats::checksum(&mut anm.rng);
            }
            CONVERTED => {
                let v = point_value(st.max_point, y, line, max_flag, st.human_max(), 10);
                let m = st.max_point / 10 - (st.max_point / 10) % 10;
                popup(v, if v >= m { POPUP_MAX } else { POPUP_NORMAL });
                st.add_score(v);
            }
            BOMB => {
                if st.bombs < 8.0 {
                    st.add_bombs(1.0, &mut anm.rng);
                }
                // Bomb item rank +5 (11 §3.2 rank deltas).
                st.rank(5);
            }
            FULL_POWER => {
                // 08 §8.5: this cancel has no spell-card test, unlike the power-item one.
                if st.power < 128.0 {
                    crate::sound::play(crate::sound::POWERUP);
                    popup(-1, POPUP_POWER);
                    out.cancel = true;
                    self.convert_power_items(anm, idx);
                }
                st.set_power(128.0, &mut anm.rng);
                popup(1000, POPUP_NORMAL);
                st.add_score(1000);
            }
            LIFE => st.extend(&mut anm.rng),
            STAR => {
                let mut v = if p.bomb_active {
                    100
                } else {
                    (st.graze_total / 40) * 10 + 300
                };
                if v < 1 {
                    v = 10;
                }
                crate::ascii::popup_special(pos, v, POPUP_NORMAL);
                st.add_score(v);
            }
            TIME_ORB => {
                let v = if p.bomb_active {
                    100
                } else if st.point_items < 2000 {
                    ((st.point_items2 / 2) * 10).max(100)
                } else {
                    10000
                };
                crate::ascii::popup_special(
                    pos,
                    v,
                    if st.time >= st.time_threshold {
                        POPUP_TIME_OVER
                    } else {
                        POPUP_TIME
                    },
                );
                st.add_score(v);
                st.add_time(1, &mut anm.rng);
                // Spell_AddBonus 0x416B10 +8000 (07 §11), applied by the enemy manager.
                st.spell_bonus_orbs += 1;
                // ±111 by player byte +3; a non-forced GaugeAdd is skipped while bombing (07 §10.3).
                if self.collect_timer == 0 && !p.bombing {
                    st.gauge_add(if p.focus != 0 { 111 } else { -111 });
                }
            }
            _ => {}
        }
        self.collected += 1;
        out
    }

    // ItemMgr_Calc 0x440500 (08 §8.4), in list order. `cancel` runs a mode-1 cancel of all bullets in the
    // middle of the loop, as the original does from inside the collection handler.
    pub fn calc(
        &mut self,
        anm: &mut AnmManager,
        p: &mut Player,
        spell: bool,
        mut cancel: impl FnMut(&mut ItemManager, &mut AnmManager, &mut Player),
    ) {
        let gs = anm.speed;
        // 08 §8.4: box C from the human sht; fall multiplier m from the sht chosen by player byte +3.
        let c = p.sht.collect;
        let m = p.item_sht().fall * gs;
        self.count = 0;
        let mut sound = None;
        let mut k = 0;
        while k < self.order.len() {
            let i = self.order[k] as usize;
            k += 1;
            if i == DEAD as usize || !self.items[i].alive {
                continue;
            }
            self.count += 1;
            let ps = p.state;
            let it = &mut self.items[i];
            let t = it.timer.cur;
            // 0 common motion, 1 collection test, 2 no test
            let mut step = 0;
            match it.state {
                2 => {
                    if t < 60 {
                        let f = t as f32 / 60.0;
                        for a in 0..3 {
                            it.pos[a] = it.vel[a] * (1.0 - f) + it.target[a] * f;
                        }
                        step = 1;
                    } else if t == 60 {
                        // 08 §8.4: state 0 only at exactly t == 60; common motion runs either way.
                        it.vel = [0.0; 3];
                        it.state = 0;
                    }
                }
                3 => {
                    it.vel[1] += 0.05 * gs;
                    // Shot timer player +0xE2AC4 < 0 = not shooting (08 §8.4).
                    if it.vel[1] > 0.0 || p.shot_timer.cur < 0 {
                        it.state = 1;
                    }
                    if ps == 2 {
                        it.state = 0;
                        it.vel = [0.0, -0.7, 0.0];
                    }
                }
                5 => {
                    it.vel[1] += 0.05 * gs;
                    for a in 0..3 {
                        it.pos[a] += it.vel[a] * m;
                    }
                    if it.vel[1] > 0.0 {
                        it.state = 1;
                        if ps == 2 {
                            it.state = 0;
                            it.vel = [0.0, -0.7, 0.0];
                        }
                    } else {
                        step = 2;
                    }
                }
                _ => {
                    // 08 §8.4: shot types 1 (Marisa & Alice) and 6 (Marisa solo) auto-collect above the line.
                    let attract = it.state == 1
                        || (p.pos[1] < p.sht.poc_line
                            && (p.stats.power >= 128.0
                                || p.focus != 0
                                || p.shot_type == 1
                                || p.shot_type == 6));
                    if attract {
                        if !matches!(ps, 1 | 2) {
                            let a = crate::bullet::angle_from(it.pos, p.pos);
                            let s = p.sht.attract_speed;
                            it.vel = crate::bullet::polar(a, s);
                            it.state = 1;
                            for a in 0..3 {
                                it.pos[a] += it.vel[a] * gs;
                            }
                            step = 1;
                        } else {
                            it.vel[1] = -0.7;
                            it.state = 0;
                        }
                    } else {
                        it.vel[0] = 0.0;
                        it.vel[2] = 0.0;
                        it.vel[1] = it.vel[1].max(-2.2);
                    }
                }
            }
            if step == 0 {
                for a in 0..3 {
                    it.pos[a] += it.vel[a] * m;
                }
                // 08 §8.4: only a falling (state 0) item below 448 + 16 is lost (rank −3).
                if it.state != 0 || it.pos[1] <= 448.0 + 16.0 {
                    if it.vel[1] >= 3.0 {
                        it.vel[1] = 3.0;
                    } else {
                        it.vel[1] += 0.03 * m;
                    }
                    step = 1;
                } else {
                    p.stats.rank(-3);
                    it.alive = false;
                    self.order[k - 1] = DEAD;
                    continue;
                }
            }
            if step == 1 && it.state != 3 && p.collect_test(it.pos, c) {
                let out = self.collect(i, anm, p, spell);
                // One sound per calc after the loop: 0x2C once any max-value item was collected, else 0x15.
                if sound != Some(crate::sound::ITEM1) {
                    sound = Some(if self.items[i].max_flag {
                        crate::sound::ITEM1
                    } else {
                        crate::sound::ITEM0
                    });
                }
                self.items[i].alive = false;
                self.order[k - 1] = DEAD;
                if out.cancel {
                    cancel(self, anm, p);
                }
                continue;
            }
            let it = &mut self.items[i];
            it.timer.tick(gs);
            if it.vm.script.is_some() {
                it.vm.run(anm);
            }
        }
        self.order
            .retain(|&i| i != DEAD && self.items[i as usize].alive);
        if let Some(id) = sound {
            crate::sound::play(id);
        }
        // 08 §8.4: gauge lock decremented and clamped at 0 after the loop.
        if self.collect_timer != 0 {
            self.collect_timer = (self.collect_timer - 1).max(0);
        }
    }

    // Item draw (08 §8.6), in list order.
    pub fn draw(&mut self, anm: &mut AnmManager, gfx: &mut Gfx) {
        for k in 0..self.order.len() {
            let it = &mut self.items[self.order[k] as usize];
            if !it.alive {
                continue;
            }
            let y = it.pos[1];
            // Above the playfield: arrow sprite at y 8, alpha fading with distance (08 §8.6).
            if y < -8.0 {
                it.vm.set_sprite(anm, 0xB6 + it.kind as i32);
                it.vm.color[anmvm::A] = (255 - ((8.0 - y) * 255.0 / 128.0) as i32).max(64) as u8;
                it.vm.pos = [it.pos[0] + 32.0, 8.0 + 16.0, 0.0];
            } else {
                it.vm.set_sprite(anm, 0xAC + it.kind as i32);
                it.vm.color[anmvm::A] = 255;
                it.vm.pos = [it.pos[0] + 32.0, y + 16.0, 0.0];
            }
            // Draw2DRot at z 0.15 (08 §8.6).
            anm.draw_mode_z(gfx, &mut it.vm, 2, 0.15);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_values() {
        assert_eq!(point_value(10000, 100.0, 128.0, false, false, 1), 10000);
        assert_eq!(point_value(10000, 300.0, 128.0, true, false, 1), 10000);
        assert_eq!(
            point_value(10000, 228.0, 128.0, false, false, 1),
            5000 - 100 * 10
        );
        assert_eq!(
            point_value(10000, 228.9, 128.0, false, true, 1),
            2 * (5000 - 100 * 10)
        );
        assert_eq!(point_value(12345, 128.5, 128.0, false, false, 1), 6170);
        assert_eq!(point_value(10000, 100.0, 128.0, false, false, 10), 1000);
        assert_eq!(point_value(12345, 228.0, 128.0, false, false, 10), 490);
    }

    #[test]
    fn extend_table() {
        assert_eq!(
            (0..8).map(|n| next_extend(1, n)).collect::<Vec<_>>(),
            [100, 250, 500, 800, 1100, 9999, 10499, 10999]
        );
        assert_eq!(
            (0..4).map(|n| next_extend(4, n)).collect::<Vec<_>>(),
            [200, 666, 9999, 99999]
        );
    }

    #[test]
    fn power_levels() {
        assert_eq!(power_level(0.0), 0);
        assert_eq!(power_level(8.0), 1);
        assert_eq!(power_level(127.0), 4);
        assert_eq!(power_level(128.0), 5);
    }

    #[test]
    fn drop_attracted_only_state_1() {
        let mut m = ItemManager::default();
        for (k, st) in [1u8, 0, 3].into_iter().enumerate() {
            let it = &mut m.items[k];
            it.alive = true;
            it.state = st;
            it.vel = [5.0, -1.0, 0.0];
        }
        m.drop_attracted();
        assert_eq!((m.items[0].state, m.items[0].vel), (0, [0.0, -0.9, 0.0]));
        assert_eq!((m.items[1].state, m.items[1].vel), (0, [5.0, -1.0, 0.0]));
        assert_eq!(m.items[2].state, 3);
    }
}
