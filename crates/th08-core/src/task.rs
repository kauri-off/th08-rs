//! Task system "funcChain" (00 §3, `FuncChain_*` 0x43C760…0x43CF50): priority-sorted calc and draw chains.

use crate::drawlist::Recorder;
use anyhow::Result;

// Callback return codes interpreted by the runners (§3 table).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Ret {
    Remove = 0,
    Next = 1,
    Repeat = 2,
    StopFrame = 3,
    Quit = 4,
    ErrorQuit = 5,
    Restart = 6,
}

// Runner result: normal (count of callbacks or 1 for code 3), 0 = quit (code 4), −1 = error quit (code 5) (§3.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    Continue,
    Quit,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chain {
    // chain+0x00, FuncChain_AddCalc 0x43C880.
    Calc,
    // chain+0x20, FuncChain_AddDraw 0x43C960.
    Draw,
}

// One subsystem object: node callbacks (0x20-byte node: +0x04 per-frame, +0x08 on-register, +0x0C on-remove).
// Port: a subsystem registers one object on several nodes; `tag` tells the nodes apart. The chain payload is the
// context's `Copy` node value (`TaskCtx::Node`), whose `Task` impl dispatches to the subsystem (chain.rs for `Ctx`).
pub trait Task<C: TaskCtx> {
    fn calc(&mut self, _ctx: &mut C, _tag: u8) -> Ret {
        Ret::Next
    }
    // Draw node: the draw-phase step makes every state write of the draw callback (logic, VM
    // caches, ascii queue) and runs whenever the draw chain runs, emit on or off.
    fn draw_step(&mut self, _ctx: &mut C, _tag: u8) -> Ret {
        Ret::Next
    }
    // Then emit: records the node's draw commands from the state the step left; reads only, skipped with emit off.
    fn emit(&self, _v: &C::View<'_>, _rec: &mut Recorder, _tag: u8) {}
    fn on_register(&mut self, _ctx: &mut C, _chain: Chain, _tag: u8) -> Result<()> {
        Ok(())
    }
    fn on_remove(&mut self, _ctx: &mut C, _chain: Chain, _tag: u8) {}
}

pub trait TaskCtx: Sized + 'static {
    // Chain payload: one value per task object (equal values = the same object, like the original's object pointer).
    type Node: Task<Self> + Copy + PartialEq + std::fmt::Debug;
    // Read-only view the emits get: nothing reachable through it can change state.
    type View<'a>;
    fn tasks(&mut self) -> &mut Tasks<Self::Node>;
    fn view(&self) -> Self::View<'_>;
    // Called when a calc node returns 6, before the walk restarts from the head (session tool: one row per pass).
    fn on_restart(&mut self) {}
    // Emit switch: the recorder emits go to, None = emit off (draw-phase steps only).
    fn recorder(&mut self) -> Option<&mut Recorder> {
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct NodeId(u64);

#[derive(Clone, Copy, Debug)]
struct Entry<N> {
    id: NodeId,
    // Node +0x00, ascending.
    prio: u16,
    tag: u8,
    task: N,
}

#[derive(Clone, Debug)]
pub struct Tasks<N> {
    calc: Vec<Entry<N>>,
    draw: Vec<Entry<N>>,
    next_id: u64,
    // Unlinked nodes whose on-remove has not run yet (see `reap`).
    dead: Vec<(N, Chain, u8)>,
    // Task objects with a callback in progress, innermost last.
    running: Vec<N>,
}

impl<N> Default for Tasks<N> {
    fn default() -> Self {
        Self {
            calc: Vec::new(),
            draw: Vec::new(),
            next_id: 0,
            dead: Vec::new(),
            running: Vec::new(),
        }
    }
}

impl<N: Copy + PartialEq> Tasks<N> {
    fn list(&mut self, chain: Chain) -> &mut Vec<Entry<N>> {
        match chain {
            Chain::Calc => &mut self.calc,
            Chain::Draw => &mut self.draw,
        }
    }

    fn view(&self, chain: Chain) -> &[Entry<N>] {
        match chain {
            Chain::Calc => &self.calc,
            Chain::Draw => &self.draw,
        }
    }

    pub fn priorities(&self, chain: Chain) -> Vec<u16> {
        self.view(chain).iter().map(|n| n.prio).collect()
    }

    // (prio, tag, task) of every node in chain order (tests, tools).
    pub fn nodes(&self, chain: Chain) -> Vec<(u16, u8, N)> {
        self.view(chain)
            .iter()
            .map(|n| (n.prio, n.tag, n.task))
            .collect()
    }

    fn busy(&self, task: N) -> bool {
        self.running.contains(&task)
    }
}

// Runs `f` with `task` marked as running (its own removal meanwhile defers its on-remove, see `reap`).
fn guarded<C: TaskCtx, R>(
    ctx: &mut C,
    task: C::Node,
    f: impl FnOnce(&mut C, &mut C::Node) -> R,
) -> R {
    ctx.tasks().running.push(task);
    let mut t = task;
    let r = f(ctx, &mut t);
    ctx.tasks().running.pop();
    r
}

// FuncChain_AddCalc / AddDraw: on-register runs first, then the node is linked after nodes of equal
// priority whatever it returned (a node it registers with the same priority lands before this one); no unlink on Err.
pub fn add<C: TaskCtx>(
    ctx: &mut C,
    chain: Chain,
    prio: u16,
    tag: u8,
    task: C::Node,
) -> Result<NodeId> {
    let id = NodeId(ctx.tasks().next_id);
    ctx.tasks().next_id += 1;
    let r = if ctx.tasks().busy(task) {
        Err(anyhow::anyhow!(
            "task registered while its own callback is running"
        ))
    } else {
        guarded(ctx, task, |ctx, t| t.on_register(ctx, chain, tag))
    };
    let list = ctx.tasks().list(chain);
    let at = list
        .iter()
        .position(|n| n.prio > prio)
        .unwrap_or(list.len());
    list.insert(
        at,
        Entry {
            id,
            prio,
            tag,
            task,
        },
    );
    r.map(|()| id)
}

// FuncChain_Remove 0x43CF50 (§3.1): unlinks exactly one node and calls its on-remove once.
pub fn remove<C: TaskCtx>(ctx: &mut C, id: NodeId) -> bool {
    for chain in [Chain::Calc, Chain::Draw] {
        if let Some(p) = ctx.tasks().list(chain).iter().position(|n| n.id == id) {
            unlink(ctx, chain, p);
            reap(ctx);
            return true;
        }
    }
    false
}

// A subsystem's unregister function (e.g. Stage 0x409CA0, §3.1): removes its calc node and all its draw nodes.
pub fn remove_task<C: TaskCtx>(ctx: &mut C, task: C::Node) {
    for chain in [Chain::Calc, Chain::Draw] {
        while let Some(p) = ctx.tasks().list(chain).iter().position(|n| n.task == task) {
            unlink(ctx, chain, p);
        }
    }
    reap(ctx);
}

// Chain teardown 0x43CE30 at the main loop's exit (00 §2): every node of both chains removed with its on-remove.
pub fn clear<C: TaskCtx>(ctx: &mut C) {
    for chain in [Chain::Calc, Chain::Draw] {
        while !ctx.tasks().list(chain).is_empty() {
            unlink(ctx, chain, 0);
        }
    }
    reap(ctx);
}

fn unlink<C: TaskCtx>(ctx: &mut C, chain: Chain, pos: usize) {
    let n = ctx.tasks().list(chain).remove(pos);
    ctx.tasks().dead.push((n.task, chain, n.tag));
}

// On-remove hooks of tasks that are currently running (removed from inside their own callback) wait for the next reap.
// Port artifact: the original calls on-remove inside FuncChain_Remove, i.e. before the caller continues.
fn reap<C: TaskCtx>(ctx: &mut C) {
    let mut i = 0;
    while i < ctx.tasks().dead.len() {
        let (task, chain, tag) = ctx.tasks().dead[i];
        if ctx.tasks().busy(task) {
            i += 1;
        } else {
            ctx.tasks().dead.remove(i);
            guarded(ctx, task, |ctx, t| t.on_remove(ctx, chain, tag));
            i = 0;
        }
    }
}

// FuncChain_RunCalc 0x43CA50 / FuncChain_RunDraw 0x43CB60.
pub fn run<C: TaskCtx>(ctx: &mut C, chain: Chain) -> Flow {
    let mut i = 0;
    loop {
        let Some((id, tag, task)) = ctx
            .tasks()
            .list(chain)
            .get(i)
            .map(|n| (n.id, n.tag, n.task))
        else {
            return Flow::Continue;
        };
        let r = guarded(ctx, task, |ctx, t| match chain {
            Chain::Calc => t.calc(ctx, tag),
            Chain::Draw => {
                let r = t.draw_step(ctx, tag);
                // The recorder leaves the context for the emit, so the emit can take a read-only view.
                if let Some(rec) = ctx.recorder() {
                    let mut rec = std::mem::replace(rec, Recorder::detached());
                    t.emit(&ctx.view(), &mut rec, tag);
                    if let Some(slot) = ctx.recorder() {
                        *slot = rec;
                    }
                }
                r
            }
        });
        reap(ctx);
        let pos = ctx.tasks().list(chain).iter().position(|n| n.id == id);
        match r {
            Ret::Remove => {
                if let Some(p) = pos {
                    unlink(ctx, chain, p);
                    reap(ctx);
                }
                i = pos.unwrap_or(i);
            }
            Ret::Repeat => i = pos.unwrap_or(i),
            Ret::StopFrame => return Flow::Continue,
            Ret::Quit => return Flow::Quit,
            Ret::ErrorQuit => return Flow::Error,
            // Restart: every calc node runs again in the same Frame_Run, Supervisor_Calc's input poll included.
            Ret::Restart if chain == Chain::Calc => {
                ctx.on_restart();
                i = 0;
            }
            // Code 6 is only special in the calc runner; the draw runner treats it like 1.
            Ret::Next | Ret::Restart => i = pos.map_or(i, |p| p + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct T {
        tasks: Tasks<N>,
        log: Vec<String>,
        rec: Option<Recorder>,
        // Per-script return codes, consumed in order (`N::Script` indexes this).
        rets: Vec<Vec<Ret>>,
    }

    // Test task objects: `Script(i, name)` returns `rets[i]` in turn, `Drawn(k)` draws, `Nest` registers a node.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum N {
        Script(usize, &'static str),
        Drawn(u8),
        Nest,
    }

    impl TaskCtx for T {
        type Node = N;
        type View<'a> = &'a T;
        fn tasks(&mut self) -> &mut Tasks<N> {
            &mut self.tasks
        }
        fn view(&self) -> &T {
            self
        }
        fn on_restart(&mut self) {
            self.log.push("R".into());
        }
        fn recorder(&mut self) -> Option<&mut Recorder> {
            self.rec.as_mut()
        }
    }

    impl Task<T> for N {
        fn calc(&mut self, ctx: &mut T, tag: u8) -> Ret {
            let N::Script(i, name) = *self else {
                return Ret::Next;
            };
            ctx.log.push(format!("{name}{tag}"));
            if ctx.rets[i].is_empty() {
                Ret::Next
            } else {
                ctx.rets[i].remove(0)
            }
        }
        fn draw_step(&mut self, ctx: &mut T, _: u8) -> Ret {
            if let N::Drawn(k) = *self {
                ctx.log.push(format!("s{k}"));
            }
            Ret::Next
        }
        fn emit(&self, ctx: &&T, rec: &mut Recorder, _: u8) {
            if let N::Drawn(k) = *self {
                rec.clear_viewport([k, ctx.log.len() as u8, 0, 0]);
            }
        }
        fn on_register(&mut self, ctx: &mut T, chain: Chain, tag: u8) -> Result<()> {
            match *self {
                N::Script(_, name) => ctx.log.push(format!("+{name}{tag}")),
                N::Nest => {
                    let inner = s(ctx, "inner", &[]);
                    add(ctx, chain, 5, 7, inner)?;
                    anyhow::bail!("fail")
                }
                N::Drawn(_) => {}
            }
            Ok(())
        }
        fn on_remove(&mut self, ctx: &mut T, _: Chain, tag: u8) {
            if let N::Script(_, name) = *self {
                ctx.log.push(format!("-{name}{tag}"));
            }
        }
    }

    fn s(c: &mut T, name: &'static str, rets: &[Ret]) -> N {
        c.rets.push(rets.to_vec());
        N::Script(c.rets.len() - 1, name)
    }

    #[test]
    fn priority_order_and_codes() {
        let mut c = T::default();
        let b = s(&mut c, "b", &[]);
        add(&mut c, Chain::Calc, 5, 0, b).unwrap();
        let a = s(&mut c, "a", &[Ret::Repeat, Ret::Next, Ret::Remove]);
        add(&mut c, Chain::Calc, 1, 0, a).unwrap();
        let x = s(&mut c, "c", &[Ret::Restart]);
        add(&mut c, Chain::Calc, 5, 1, x).unwrap();
        let d = s(&mut c, "d", &[Ret::StopFrame, Ret::Quit, Ret::ErrorQuit]);
        add(&mut c, Chain::Calc, 9, 0, d).unwrap();
        assert_eq!(c.tasks.priorities(Chain::Calc), [1, 5, 5, 9]);
        c.log.clear();
        assert_eq!(run(&mut c, Chain::Calc), Flow::Continue);
        assert_eq!(c.log.join(" "), "a0 a0 b0 c1 R a0 -a0 b0 c1 d0");
        c.log.clear();
        assert_eq!(run(&mut c, Chain::Calc), Flow::Quit);
        assert_eq!(run(&mut c, Chain::Calc), Flow::Error);
        assert_eq!(run(&mut c, Chain::Calc), Flow::Continue);
        assert_eq!(c.tasks.priorities(Chain::Calc), [5, 5, 9]);
    }

    // Restart / exit (00 §2): both chains emptied, every node's on-remove called (calc chain first).
    #[test]
    fn clear_all_chains() {
        let mut c = T::default();
        let m = s(&mut c, "m", &[]);
        add(&mut c, Chain::Calc, 4, 0, m).unwrap();
        let n = s(&mut c, "n", &[]);
        add(&mut c, Chain::Draw, 3, 3, n).unwrap();
        c.log.clear();
        clear(&mut c);
        assert_eq!(c.log.join(" "), "-m0 -n3");
        assert!(c.tasks.priorities(Chain::Calc).is_empty());
        assert!(c.tasks.priorities(Chain::Draw).is_empty());
    }

    #[test]
    fn remove_whole_task() {
        let mut c = T::default();
        let t = s(&mut c, "x", &[]);
        add(&mut c, Chain::Calc, 0, 0, t).unwrap();
        add(&mut c, Chain::Draw, 2, 2, t).unwrap();
        remove_task(&mut c, t);
        assert_eq!(c.log.join(" "), "+x0 +x2 -x0 -x2");
        assert!(c.tasks.priorities(Chain::Draw).is_empty());
    }

    // Each draw node's step, then its emit (seeing that step's writes), in chain order; emit off runs
    // the same steps and records nothing.
    #[test]
    fn draw_step_then_emit() {
        use crate::drawlist::DrawCmd;
        let mut c = T {
            rec: Some(Recorder::new()),
            ..T::default()
        };
        add(&mut c, Chain::Draw, 2, 0, N::Drawn(2)).unwrap();
        add(&mut c, Chain::Draw, 1, 0, N::Drawn(1)).unwrap();
        run(&mut c, Chain::Draw);
        assert_eq!(c.log.join(" "), "s1 s2");
        let draws = c.rec.as_mut().unwrap().take().0;
        assert!(matches!(
            draws[..],
            [
                DrawCmd::ClearViewport([1, 1, 0, 0]),
                DrawCmd::ClearViewport([2, 2, 0, 0])
            ]
        ));
        c.rec = None;
        c.log.clear();
        run(&mut c, Chain::Draw);
        assert_eq!(c.log.join(" "), "s1 s2");
    }

    // A node registered by on-register with the same priority lands first; the outer node stays on Err.
    #[test]
    fn add_runs_on_register_before_linking() {
        let mut c = T::default();
        assert!(add(&mut c, Chain::Calc, 5, 1, N::Nest).is_err());
        let tags: Vec<u8> = c.tasks.calc.iter().map(|n| n.tag).collect();
        assert_eq!(tags, [7, 1]);
    }

    // Code 0 unlinks only the running node (its draw node stays); the chain is a plain value that clones.
    #[test]
    fn remove_code_and_chain_clone() {
        let mut c = T::default();
        let a = s(&mut c, "a", &[Ret::Remove]);
        add(&mut c, Chain::Calc, 1, 0, a).unwrap();
        add(&mut c, Chain::Draw, 1, 4, a).unwrap();
        let saved = c.tasks.clone();
        c.log.clear();
        run(&mut c, Chain::Calc);
        assert_eq!(c.log.join(" "), "a0 -a0");
        assert_eq!(c.tasks.nodes(Chain::Draw), [(1, 4, a)]);
        c.tasks = saved;
        assert_eq!(c.tasks.nodes(Chain::Calc), [(1, 0, a)]);
    }
}
