// SPDX-License-Identifier: GPL-3.0-or-later

//! VM scheduler, extracted from the interpreter described in docs/papyrus.md.
//! The public API remains on Vm and uses the same single scheduler.

use super::{Frame, LatentId, Thread, ThreadId, ThreadState, Value, Vm};

impl Vm {
    pub(super) fn spawn(&mut self, frame: Frame) -> ThreadId {
        self.next_thread += 1;
        let id = ThreadId(self.next_thread);
        self.threads.push(Thread {
            id,
            frames: vec![frame],
            state: ThreadState::Runnable,
            result: Value::None,
            pending_dest: None,
        });
        id
    }

    pub fn thread_state(&self, id: ThreadId) -> Option<ThreadState> {
        self.threads
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.state.clone())
    }

    /// The return value of a finished thread.
    pub fn thread_result(&self, id: ThreadId) -> Option<Value> {
        self.threads
            .iter()
            .find(|t| t.id == id && t.state == ThreadState::Done)
            .map(|t| t.result.clone())
    }

    pub fn idle(&self) -> bool {
        self.threads
            .iter()
            .all(|t| t.state != ThreadState::Runnable)
    }

    pub fn live_threads(&self) -> usize {
        self.threads
            .iter()
            .filter(|t| t.state != ThreadState::Done)
            .count()
    }

    /// Finish a latent native. The waiting thread receives the value and runs
    /// again on the next `run`.
    pub fn resolve_latent(&mut self, id: LatentId, value: Value) -> bool {
        let Some(tid) = self.latent_waiters.remove(&id) else {
            return false;
        };
        let Some(t) = self.threads.iter_mut().find(|t| t.id == tid) else {
            return false;
        };
        if let Some(dest) = t.pending_dest.take() {
            let idx = t.frames.len() - 1;
            let frame = &mut t.frames[idx];
            Self::store(&mut self.instances, frame, &dest, value);
        }
        t.state = ThreadState::Runnable;
        true
    }

    /// Move the clock and wake sleepers whose time has come.
    pub fn advance(&mut self, seconds: f64) {
        self.time += seconds;
        for t in &mut self.threads {
            if let ThreadState::Sleeping(until) = t.state {
                if until <= self.time {
                    t.state = ThreadState::Runnable;
                }
            }
        }
    }

    /// Run runnable threads round-robin until the budget of instructions is
    /// spent or nothing is runnable. Returns instructions executed.
    pub fn run(&mut self, budget: usize) -> usize {
        let mut run_span = soe_trace::Span::new("vm.run", "vm");
        let slice = 64;
        let mut used = 0;
        while used < budget {
            let mut progressed = false;
            for i in 0..self.threads.len() {
                if self.threads[i].state != ThreadState::Runnable {
                    continue;
                }
                let _slice_span = soe_trace::Span::new("vm.slice", "vm");
                let n = self.step_thread(i, slice.min(budget - used));
                run_span.set_payload(used as u64 + n as u64);
                used += n;
                progressed |= n > 0;
                if used >= budget {
                    break;
                }
            }
            if !progressed {
                break;
            }
        }
        used
    }

    /// Run until no thread is runnable, at most `max` instructions. Sleeping
    /// threads are not woken, call [`Vm::advance`] for that. Returns whether
    /// the VM went idle.
    pub fn run_until_idle(&mut self, max: usize) -> bool {
        self.run(max);
        self.idle()
    }
}
