//! Task system "funcChain" (00 §3, `FuncChain_*` 0x43C760…0x43CF50): priority-sorted calc and draw chains.

use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

// Callback return codes interpreted by the runners (00 §3 table).
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

// Runner result: normal (count of callbacks or 1 for code 3), 0 = quit (code 4), −1 = error quit (code 5) (00 §3.1).
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

// One subsystem object: node callbacks (0x20-byte node: +0x04 per-frame, +0x08 on-register, +0x0C on-remove, 00 §3).
// Port: a subsystem registers one object on several nodes; `tag` tells the nodes apart.
pub trait Task<C> {
    fn calc(&mut self, _ctx: &mut C, _tag: u8) -> Ret {
        Ret::Next
    }
    fn draw(&mut self, _ctx: &mut C, _tag: u8) -> Ret {
        Ret::Next
    }
    fn on_register(&mut self, _ctx: &mut C, _chain: Chain, _tag: u8) -> Result<()> {
        Ok(())
    }
    fn on_remove(&mut self, _ctx: &mut C, _chain: Chain, _tag: u8) {}
}

pub type TaskRef<C> = Rc<RefCell<dyn Task<C>>>;

pub trait TaskCtx: Sized + 'static {
    fn tasks(&mut self) -> &mut Tasks<Self>;
}

pub fn task<C, T: Task<C> + 'static>(t: T) -> TaskRef<C> {
    Rc::new(RefCell::new(t))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct NodeId(u64);

struct Node<C> {
    id: NodeId,
    // Node +0x00, ascending.
    prio: u16,
    tag: u8,
    task: TaskRef<C>,
}

pub struct Tasks<C> {
    calc: Vec<Node<C>>,
    draw: Vec<Node<C>>,
    next_id: u64,
    // Unlinked nodes whose on-remove has not run yet (see `reap`).
    dead: Vec<(TaskRef<C>, Chain, u8)>,
}

impl<C> Default for Tasks<C> {
    fn default() -> Self {
        Self {
            calc: Vec::new(),
            draw: Vec::new(),
            next_id: 0,
            dead: Vec::new(),
        }
    }
}

impl<C> Tasks<C> {
    fn list(&mut self, chain: Chain) -> &mut Vec<Node<C>> {
        match chain {
            Chain::Calc => &mut self.calc,
            Chain::Draw => &mut self.draw,
        }
    }

    pub fn priorities(&self, chain: Chain) -> Vec<u16> {
        let l = if chain == Chain::Calc {
            &self.calc
        } else {
            &self.draw
        };
        l.iter().map(|n| n.prio).collect()
    }
}

// FuncChain_AddCalc / AddDraw (00 §3): on-register runs first, then the node is linked after nodes of equal
// priority whatever it returned (a node it registers with the same priority lands before this one); no unlink on Err.
pub fn add<C: TaskCtx>(
    ctx: &mut C,
    chain: Chain,
    prio: u16,
    tag: u8,
    task: TaskRef<C>,
) -> Result<NodeId> {
    let id = NodeId(ctx.tasks().next_id);
    ctx.tasks().next_id += 1;
    let r = match task.try_borrow_mut() {
        Ok(mut t) => t.on_register(ctx, chain, tag),
        Err(_) => Err(anyhow::anyhow!(
            "task registered while its own callback is running"
        )),
    };
    let list = ctx.tasks().list(chain);
    let at = list
        .iter()
        .position(|n| n.prio > prio)
        .unwrap_or(list.len());
    list.insert(
        at,
        Node {
            id,
            prio,
            tag,
            task,
        },
    );
    r.map(|()| id)
}

// FuncChain_Remove 0x43CF50 (00 §3.1): unlinks exactly one node and calls its on-remove once.
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

// A subsystem's unregister function (e.g. Stage 0x409CA0, 00 §3.1): removes its calc node and all its draw nodes.
pub fn remove_task<C: TaskCtx>(ctx: &mut C, task: &TaskRef<C>) {
    for chain in [Chain::Calc, Chain::Draw] {
        while let Some(p) = ctx
            .tasks()
            .list(chain)
            .iter()
            .position(|n| Rc::ptr_eq(&n.task, task))
        {
            unlink(ctx, chain, p);
        }
    }
    reap(ctx);
}

fn unlink<C: TaskCtx>(ctx: &mut C, chain: Chain, pos: usize) {
    let n = ctx.tasks().list(chain).remove(pos);
    ctx.tasks().dead.push((n.task, chain, n.tag));
}

// On-remove hooks of tasks that are currently borrowed (removed from inside their own callback) wait for the next reap.
// Port artifact (RefCell): the original calls on-remove inside FuncChain_Remove, i.e. before the caller continues.
fn reap<C: TaskCtx>(ctx: &mut C) {
    let mut i = 0;
    while i < ctx.tasks().dead.len() {
        let (task, chain, tag) = ctx.tasks().dead[i].clone();
        match task.try_borrow_mut() {
            Ok(mut t) => {
                ctx.tasks().dead.remove(i);
                t.on_remove(ctx, chain, tag);
                i = 0;
            }
            Err(_) => i += 1,
        }
    }
}

// FuncChain_RunCalc 0x43CA50 / FuncChain_RunDraw 0x43CB60 (00 §3, §3.1).
pub fn run<C: TaskCtx>(ctx: &mut C, chain: Chain) -> Flow {
    let mut i = 0;
    loop {
        let Some((id, tag, task)) = ctx
            .tasks()
            .list(chain)
            .get(i)
            .map(|n| (n.id, n.tag, n.task.clone()))
        else {
            return Flow::Continue;
        };
        let r = {
            let mut t = task.borrow_mut();
            match chain {
                Chain::Calc => t.calc(ctx, tag),
                Chain::Draw => t.draw(ctx, tag),
            }
        };
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
            Ret::Restart if chain == Chain::Calc => i = 0,
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
        tasks: Tasks<T>,
        log: Vec<String>,
    }

    impl TaskCtx for T {
        fn tasks(&mut self) -> &mut Tasks<Self> {
            &mut self.tasks
        }
    }

    struct Script {
        name: &'static str,
        rets: Vec<Ret>,
    }

    impl Task<T> for Script {
        fn calc(&mut self, ctx: &mut T, tag: u8) -> Ret {
            ctx.log.push(format!("{}{tag}", self.name));
            if self.rets.is_empty() {
                Ret::Next
            } else {
                self.rets.remove(0)
            }
        }
        fn on_register(&mut self, ctx: &mut T, _: Chain, tag: u8) -> Result<()> {
            ctx.log.push(format!("+{}{tag}", self.name));
            Ok(())
        }
        fn on_remove(&mut self, ctx: &mut T, _: Chain, tag: u8) {
            ctx.log.push(format!("-{}{tag}", self.name));
        }
    }

    fn s(name: &'static str, rets: &[Ret]) -> TaskRef<T> {
        task(Script {
            name,
            rets: rets.to_vec(),
        })
    }

    #[test]
    fn priority_order_and_codes() {
        let mut c = T::default();
        add(&mut c, Chain::Calc, 5, 0, s("b", &[])).unwrap();
        add(
            &mut c,
            Chain::Calc,
            1,
            0,
            s("a", &[Ret::Repeat, Ret::Next, Ret::Remove]),
        )
        .unwrap();
        add(&mut c, Chain::Calc, 5, 1, s("c", &[Ret::Restart])).unwrap();
        add(
            &mut c,
            Chain::Calc,
            9,
            0,
            s("d", &[Ret::StopFrame, Ret::Quit, Ret::ErrorQuit]),
        )
        .unwrap();
        assert_eq!(c.tasks.priorities(Chain::Calc), [1, 5, 5, 9]);
        c.log.clear();
        assert_eq!(run(&mut c, Chain::Calc), Flow::Continue);
        assert_eq!(c.log.join(" "), "a0 a0 b0 c1 a0 -a0 b0 c1 d0");
        c.log.clear();
        assert_eq!(run(&mut c, Chain::Calc), Flow::Quit);
        assert_eq!(run(&mut c, Chain::Calc), Flow::Error);
        assert_eq!(run(&mut c, Chain::Calc), Flow::Continue);
        assert_eq!(c.tasks.priorities(Chain::Calc), [5, 5, 9]);
    }

    #[test]
    fn remove_whole_task() {
        let mut c = T::default();
        let t = s("x", &[]);
        add(&mut c, Chain::Calc, 0, 0, t.clone()).unwrap();
        add(&mut c, Chain::Draw, 2, 2, t.clone()).unwrap();
        remove_task(&mut c, &t);
        assert_eq!(c.log.join(" "), "+x0 +x2 -x0 -x2");
        assert!(c.tasks.priorities(Chain::Draw).is_empty());
    }

    struct Nest;

    impl Task<T> for Nest {
        fn on_register(&mut self, ctx: &mut T, chain: Chain, _: u8) -> Result<()> {
            add(ctx, chain, 5, 7, s("inner", &[]))?;
            anyhow::bail!("fail")
        }
    }

    // 00 §3: a node registered by on-register with the same priority lands first; the outer node stays on Err.
    #[test]
    fn add_runs_on_register_before_linking() {
        let mut c = T::default();
        assert!(add(&mut c, Chain::Calc, 5, 1, task(Nest)).is_err());
        let tags: Vec<u8> = c.tasks.calc.iter().map(|n| n.tag).collect();
        assert_eq!(tags, [7, 1]);
    }
}
