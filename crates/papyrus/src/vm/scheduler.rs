// SPDX-License-Identifier: GPL-3.0-or-later

//! VM scheduler, extracted from the interpreter described in docs/papyrus.md.
//! The public API remains on Vm and uses the same single scheduler.

use super::{
    Frame, LatentId, Thread, ThreadId, ThreadState, Value, Vm, MAX_COMPLETED_THREAD_HISTORY,
};

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

    /// Advance without a menu at Skyrim's default timescale of 20.
    pub fn advance(&mut self, seconds: f64) {
        self.advance_clocks(seconds, seconds.max(0.0) * 20.0 / 3600.0, false);
    }

    /// Advance each wait domain. The host supplies elapsed game hours and
    /// whether gameplay is paused by a menu. Menu waits always see real time.
    pub fn advance_clocks(&mut self, real_seconds: f64, game_hours: f64, menu_mode: bool) {
        let seconds = real_seconds.max(0.0);
        self.menu_time += seconds;
        if !menu_mode {
            self.time += seconds;
            self.game_hours += game_hours.max(0.0);
        }
        for t in &mut self.threads {
            let ready = match t.state {
                ThreadState::Sleeping(until) => until <= self.time,
                ThreadState::SleepingMenu(until) => until <= self.menu_time,
                ThreadState::SleepingGame(until) => until <= self.game_hours,
                _ => false,
            };
            if ready {
                t.state = ThreadState::Runnable;
            }
        }
    }

    /// Run runnable threads round-robin until the budget of instructions is
    /// spent or nothing is runnable. Returns instructions executed.
    pub fn run(&mut self, budget: usize) -> usize {
        if self.suspended_state.is_some() {
            return 0;
        }
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
        self.retain_recent_completed_threads();
        used
    }

    fn retain_recent_completed_threads(&mut self) {
        let completed = self
            .threads
            .iter()
            .filter(|thread| thread.state == ThreadState::Done)
            .count();
        if completed <= MAX_COMPLETED_THREAD_HISTORY {
            return;
        }

        let first_retained = self
            .threads
            .iter()
            .filter(|thread| thread.state == ThreadState::Done)
            .nth(completed - MAX_COMPLETED_THREAD_HISTORY)
            .map(|thread| thread.id);
        if let Some(first_retained) = first_retained {
            self.threads.retain(|thread| {
                thread.state != ThreadState::Done || thread.id.0 >= first_retained.0
            });
        }
    }

    /// Run until no thread is runnable, at most `max` instructions. Sleeping
    /// threads are not woken, call [`Vm::advance`] for that. Returns whether
    /// the VM went idle.
    pub fn run_until_idle(&mut self, max: usize) -> bool {
        self.run(max);
        self.idle()
    }
}
