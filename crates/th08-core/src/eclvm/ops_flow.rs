//! ECL control flow: wait, jumps, calls, returns, threads, interrupts, natives (07 §5.2–§5.5, §7).

use super::{Exec, Flow, at, at_mut};
use crate::enemy::{A_NO_PUSH, Thread, V_ARGI};
use std::cmp::Ordering::{Equal, Greater, Less};
use th08_formats::ecl::{Arg, Cmp, FlowOp, Ins};

impl Exec<'_> {
    pub(super) fn op_flow(&mut self, t: &mut Thread, ins: &Ins, op: FlowOp) -> Flow {
        let next = ins.off.idx() + ins.size;
        match op {
            FlowOp::Nop => {}
            FlowOp::Delete => return Flow::Delete,
            FlowOp::Wait(n) => {
                let n = self.ai(t, n);
                t.ctx.wait.set(n);
            }
            FlowOp::Jump { time, to } => return self.jump(t, ins, time, to),
            // Op 5: counter − 1 through the destination reference; jump while > 0.
            FlowOp::Loop { time, to, counter } => {
                let c = self.ai_dst(t, counter).wrapping_sub(1);
                self.set_i(t, ins, counter, c);
                if c > 0 {
                    return self.jump(t, ins, time, to);
                }
            }
            // Ops 40–51: even ops int, odd ops float compares (NaN taken as Greater).
            FlowOp::JumpIf {
                cmp,
                float,
                a,
                b,
                time,
                to,
            } => {
                let ord = if float {
                    self.af(t, a).partial_cmp(&self.af(t, b)).unwrap_or(Greater)
                } else {
                    self.ai(t, a).cmp(&self.ai(t, b))
                };
                let take = match cmp {
                    Cmp::Eq => ord == Equal,
                    Cmp::Ne => ord != Equal,
                    Cmp::Lt => ord == Less,
                    Cmp::Le => ord != Greater,
                    Cmp::Gt => ord == Greater,
                    Cmp::Ge => ord != Less,
                };
                if take {
                    return self.jump(t, ins, time, to);
                }
            }
            FlowOp::Call { sub } => {
                let sub = self.ai(t, sub);
                if self.m.ecl.sub_start(sub).is_some() {
                    self.call(t, next, sub);
                    self.loaded_ip = t.ctx.ip;
                    return Flow::Jumped;
                }
            }
            FlowOp::Return => return self.ret(t),
            FlowOp::CallBoss { slot, sub } => self.call_boss(t, slot, sub),
            FlowOp::InterruptBoss { slot, k } => {
                let (slot, k) = (self.ai(t, slot), self.ai(t, k));
                match self.m.boss(slot) {
                    Some(b) if b == self.idx => self.e.pending_interrupt = k,
                    Some(b) => self.m.enemies[b].pending_interrupt = k,
                    None => {}
                }
            }
            FlowOp::InterruptNow(k) => {
                self.e.pending_interrupt = self.ai(t, k);
                return Flow::Interrupt;
            }
            FlowOp::SetInterrupt { sub, k } => {
                let (sub, k) = (self.ai(t, sub), self.ai(t, k));
                if let Some(s) = at_mut(&mut self.e.interrupts, k) {
                    *s = sub;
                }
            }
            FlowOp::Thread { k, sub } => self.thread(t, k, sub),
            FlowOp::Native { n, arg } => {
                let (n, arg) = (self.ai(t, n), self.ai(t, arg));
                self.native(t, n, arg);
            }
            FlowOp::NativeCb { n, arg } => {
                let (n, arg) = (self.ai(t, n), self.ai(t, arg));
                t.ctx.callback = if n < 0 { -1 } else { n };
                t.ctx.callback_arg = arg;
            }
            // Op 146: context clock += n.
            FlowOp::TimeAdd(n) => {
                let n = self.ai(t, n);
                t.ctx.clock.cur += n;
            }
        }
        Flow::Next
    }

    // Op 53, 0x421CB0: restore the whole context from stack[depth].
    fn ret(&mut self, t: &mut Thread) -> Flow {
        t.depth -= 1;
        if t.depth < 0 {
            if self.e.running_thread.is_some() {
                return Flow::ThreadEnd;
            }
            // Malformed main-context return at depth 0 (never in shipped data): halt this frame.
            t.depth = 0;
            return Flow::Post;
        }
        match at(&t.stack, t.depth) {
            Some(c) => t.ctx = c.clone(),
            None => return Flow::Post,
        }
        self.loaded_ip = t.ctx.ip;
        Flow::Jumped
    }

    // Op 88: call in boss[slot]'s main context the same way.
    fn call_boss(&mut self, t: &mut Thread, slot: Arg, sub: Arg) {
        let (slot, sub) = (self.ai(t, slot), self.ai(t, sub));
        let Some(b) = self.m.boss(slot) else {
            return;
        };
        if self.m.ecl.sub_start(sub).is_none() {
            return;
        }
        let args = self.m.g.args;
        let ecl = &self.m.ecl;
        if b == self.idx {
            // On the caller itself: the running context, its own stack and working depth; the
            // pushed ip is the stored ip + its size. Ecl_Run then writes its local ip (past op 88) back
            // over the sub start, so the sub never runs; clock, wait, sub id, args and the push stay.
            let cur = t.ctx.ip;
            t.ctx.ip = self.loaded_ip + ecl.ins(self.loaded_ip).map_or(0, |i| i.size);
            if self.e.flags & A_NO_PUSH == 0 {
                t.push();
            }
            t.ctx.enter(ecl, sub);
            t.ctx.vars[V_ARGI..V_ARGI + 8].copy_from_slice(&args);
            t.ctx.ip = cur;
            return;
        }
        let o = &mut self.m.enemies[b];
        // Ecl_CallSub gets the target's stored ip, so the instruction it would run next is
        // skipped on return; the push goes to the working depth 0x2CEA, the main depth 0x2CE8 stays.
        let ip = o.main.ctx.ip;
        o.main.ctx.ip = ip + ecl.ins(ip).map_or(0, |i| i.size);
        if o.flags & A_NO_PUSH == 0 {
            o.work_depth = o.main.push_at(o.work_depth);
        }
        o.main.ctx.enter(ecl, sub);
        o.main.ctx.vars[V_ARGI..V_ARGI + 8].copy_from_slice(&args);
    }

    // Op 135: free thread slot k; sub ≥ 0 creates it with the current context's 30 var dwords, clock 0.
    fn thread(&mut self, t: &Thread, k: Arg, sub: Arg) {
        let (k, sub) = (self.ai(t, k), self.ai(t, sub));
        let Some(k) = usize::try_from(k).ok().filter(|&k| k < 4) else {
            return;
        };
        if self.e.running_thread == Some(k) {
            self.e.kill_running_thread = true;
        }
        self.e.threads[k] = None;
        if sub >= 0 {
            let mut th = Thread::default();
            th.ctx.vars = t.ctx.vars;
            if th.ctx.enter(&self.m.ecl, sub) {
                self.e.threads[k] = Some(Box::new(th));
                if self.e.running_thread == Some(k) {
                    self.e.kill_running_thread = false;
                }
            }
        }
    }
}
