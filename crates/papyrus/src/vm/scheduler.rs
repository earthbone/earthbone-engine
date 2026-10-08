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
            if soe_trace::Profile::enabled()
                && !matches!(t.state, ThreadState::Runnable | ThreadState::Done)
            {
                if let Some(frame) = t.frames.last() {
                    soe_trace::Profile::wait(
                        || {
                            let script = frame
                                .script
                                .as_ref()
                                .map_or("<native>", |script| script.name.as_str());
                            let plugin = self
                                .script_plugins
                                .get(&script.to_ascii_lowercase())
                                .map_or("<unknown>", String::as_str);
                            soe_trace::Entity::new(
                                "script",
                                format!("{script}.{}", frame.function.name),
                                plugin,
                                "latent_wait",
                            )
                        },
                        seconds,
                        0,
                    );
                }
            }
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
        let mut newly_completed = Vec::new();
        while used < budget {
            let mut progressed = false;
            for i in 0..self.threads.len() {
                if self.threads[i].state != ThreadState::Runnable {
                    continue;
                }
                let _slice_span = soe_trace::Span::new("vm.slice", "vm");
                let n = self.step_thread(i, slice.min(budget - used));
                if self.threads[i].state == ThreadState::Done {
                    newly_completed.push(self.threads[i].id);
                }
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
        self.retain_recent_completed_threads(&newly_completed);
        used
    }

    /// Run the existing interpreter until an instruction allowance or wall
    /// deadline expires. `next` retains round-robin position across slices.
    /// A native already executing completes before the deadline is checked.
    /// The cursor is host scheduling data and is not serialized by the VM.
    pub fn run_before(
        &mut self,
        budget: usize,
        deadline: std::time::Instant,
        next: &mut Option<ThreadId>,
    ) -> usize {
        if self.suspended_state.is_some() || self.threads.is_empty() {
            return 0;
        }
        let _span = soe_trace::Span::new("vm.run", "vm");
        let mut used = 0;
        let mut completed = Vec::new();
        let mut index = next
            .and_then(|id| self.threads.iter().position(|thread| thread.id == id))
            .unwrap_or(0);
        let mut visited_without_progress = 0;
        while used < budget && std::time::Instant::now() < deadline {
            let executed = if self.threads[index].state == ThreadState::Runnable {
                let _span = soe_trace::Span::new("vm.slice", "vm");
                let executed =
                    self.step_thread_before(index, 64.min(budget - used), Some(deadline));
                if self.threads[index].state == ThreadState::Done {
                    completed.push(self.threads[index].id);
                }
                executed
            } else {
                0
            };
            used += executed;
            visited_without_progress = if executed == 0 {
                visited_without_progress + 1
            } else {
                0
            };
            index = (index + 1) % self.threads.len();
            *next = Some(self.threads[index].id);
            if visited_without_progress >= self.threads.len() {
                break;
            }
        }
        self.retain_recent_completed_threads(&completed);
        used
    }

    fn retain_recent_completed_threads(&mut self, newly_completed: &[ThreadId]) {
        if newly_completed.is_empty() {
            return;
        }

        let newly_completed_set: std::collections::HashSet<_> =
            newly_completed.iter().copied().collect();
        let mut completion_order: Vec<_> = self
            .threads
            .iter()
            .filter(|thread| thread.state == ThreadState::Done)
            .map(|thread| thread.id)
            .filter(|id| !newly_completed_set.contains(id))
            .collect();
        completion_order.extend_from_slice(newly_completed);

        let ranks: std::collections::HashMap<_, _> = completion_order
            .iter()
            .enumerate()
            .map(|(rank, id)| (*id, rank))
            .collect();
        let mut completed = Vec::new();
        let mut active = Vec::new();
        for thread in self.threads.drain(..) {
            if thread.state == ThreadState::Done {
                completed.push(thread);
            } else {
                active.push(thread);
            }
        }
        completed.sort_by_key(|thread| ranks[&thread.id]);

        let expired = completed.len().saturating_sub(MAX_COMPLETED_THREAD_HISTORY);
        let mut retained = completed.into_iter().skip(expired);
        active.extend(retained.by_ref());
        self.threads = active;

        // Completed threads stay at the end in completion order. Save/load
        // preserves that order, so later completions can evict the oldest.
        let completed_count = self
            .threads
            .iter()
            .filter(|thread| thread.state == ThreadState::Done)
            .count();
        if completed_count > 0 {
            let split = self.threads.len() - completed_count;
            self.threads[split..].sort_by_key(|thread| ranks[&thread.id]);
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
