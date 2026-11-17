//! Concept models of the Zephyr real-time operating system.
//!
//! The rules come from the Zephyr documentation (Kernel Services: Threads, Scheduling)
//! and `kernel/Kconfig`. They are quoted next to the code that implements them.

// ---------------------------------------------------------------------------------
// Threads: cooperative, preemptible, and time slicing
// ---------------------------------------------------------------------------------

/// A thread written as the usual endless loop: work for `work_ms`, then `k_sleep`
/// for `sleep_ms`, and again.
#[derive(Clone, Debug, PartialEq)]
pub struct Thread {
    pub name: String,
    /// "Numerically lower priorities takes precedence over numerically higher values."
    /// "A cooperative thread has a negative priority value. A preemptible thread has a
    /// non-negative priority value."
    pub priority: i8,
    /// Processor time one pass of the loop needs.
    pub work_ms: f64,
    /// How long the thread sleeps after each pass. 0 = it never sleeps.
    pub sleep_ms: f64,
    pub enabled: bool,
}

impl Thread {
    pub fn is_cooperative(&self) -> bool {
        self.priority < 0
    }
}

/// `CONFIG_TIMESLICING` and `CONFIG_TIMESLICE_SIZE` (milliseconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slicing {
    pub enabled: bool,
    pub slice_ms: f64,
}

/// A stretch of time. In `Trace::running`, `who` is 0 for the idle thread and `i + 1`
/// for thread `i`; in `Trace::ready` it is always `i + 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub who: usize,
    pub start_ms: f64,
    pub end_ms: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThreadStats {
    /// Processor time the thread got.
    pub cpu_ms: f64,
    /// How many times it was given the processor.
    pub runs: u32,
    /// Longest time from becoming ready (start or wake-up) to getting the processor.
    pub worst_wait_ms: f64,
    /// True if it was still waiting for the processor when the simulation ended; that
    /// unfinished wait is included in `worst_wait_ms`.
    pub waiting_at_end: bool,
    /// Times a more urgent thread took the processor away from it.
    pub preempted: u32,
    /// Times its time slice ran out and a thread of equal priority took over.
    pub sliced: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    /// Who had the processor.
    pub running: Vec<Span>,
    /// When each thread was ready but did not have the processor.
    pub ready: Vec<Span>,
    /// `(thread, time)`: the moments a thread woke up from its sleep.
    pub wakes: Vec<(usize, f64)>,
    pub stats: Vec<ThreadStats>,
    /// Share of the time no thread was ready (the idle thread ran), 0..=1.
    pub idle_share: f64,
}

#[derive(Clone, Copy)]
struct State {
    ready: bool,
    wake_at: f64,
    work_left: f64,
    /// Position in the queue of ready threads: a lower number has waited longer.
    seq: u64,
    /// Set while the thread has become ready and has not had the processor since.
    ready_since: Option<f64>,
}

const EPS: f64 = 1e-9;

/// Simulate Zephyr's scheduler on one processor for `duration_ms`. All threads are
/// ready at time 0. Rules modelled, quoted from the documentation:
///
/// - "The kernel's scheduler selects the highest priority ready thread to be the
///   current thread. When multiple ready threads of the same priority exist, the
///   scheduler chooses the one that has been waiting longest."
/// - "Once a cooperative thread becomes the current thread, it remains the current
///   thread until it performs an action that makes it unready."
/// - A preemptible thread "remains the current thread until a higher priority thread
///   becomes ready, or until the thread performs an action that makes it unready."
/// - Time slicing: "At the end of every time slice, the scheduler checks to see if the
///   current thread is preemptible and, if so, implicitly invokes k_yield() on behalf of
///   the thread." `k_yield()` "puts the thread at the back of the scheduler's
///   prioritized list of ready threads". "Scheduling a new thread causes the time
///   slice timer to be reset."
///
/// One detail is not in the documentation and is taken from the kernel source for a
/// single processor (`kernel/sched.c`): the running thread stays in the queue of ready
/// threads ("In uniprocessor mode, we can leave the current thread in the queue"), so a
/// preemptible thread that is pushed aside by a more urgent one keeps its place ahead of
/// its equals and continues first afterwards - and because it is then scheduled anew
/// (`update_cache`: `if (thread != _current) z_time_slice_reset(thread)`), it gets a
/// whole new time slice.
pub fn simulate(threads: &[Thread], slicing: Slicing, duration_ms: f64) -> Trace {
    let n = threads.len();
    let mut trace = Trace { stats: vec![ThreadStats::default(); n], ..Default::default() };
    let mut st: Vec<State> = threads
        .iter()
        .enumerate()
        .map(|(i, t)| State { ready: t.enabled, wake_at: f64::INFINITY, work_left: t.work_ms.max(0.05), seq: i as u64, ready_since: t.enabled.then_some(0.0) })
        .collect();
    let mut next_seq = n as u64;
    let mut last_ready: Vec<Option<usize>> = vec![None; n];
    let mut current: Option<usize> = None;
    let mut slice_start = 0.0;
    let mut yielded = false;
    let mut t = 0.0f64;
    let mut idle = 0.0;
    let mut guard = 0;

    while t < duration_ms - EPS && guard < 200_000 {
        guard += 1;
        // 1. Sleeping threads whose time is up become ready, at the back of the queue.
        for (i, s) in st.iter_mut().enumerate() {
            if !s.ready && s.wake_at <= t + EPS {
                *s = State { ready: true, wake_at: f64::INFINITY, seq: next_seq, ready_since: Some(t), ..*s };
                next_seq += 1;
                trace.wakes.push((i, t));
            }
        }
        // 2. Pick the current thread.
        let best = (0..n).filter(|&i| st[i].ready).min_by_key(|&i| (threads[i].priority, st[i].seq));
        let next = match current {
            Some(c) if st[c].ready && threads[c].is_cooperative() => Some(c),
            _ => best,
        };
        if next != current {
            if let Some(c) = current {
                if st[c].ready && !yielded {
                    trace.stats[c].preempted += 1;
                }
            }
            if let Some(i) = next {
                trace.stats[i].runs += 1;
            }
            slice_start = t;
            current = next;
        }
        yielded = false;
        if let Some(c) = current {
            if let Some(since) = st[c].ready_since.take() {
                trace.stats[c].worst_wait_ms = trace.stats[c].worst_wait_ms.max(t - since);
            }
        }
        // 3. Run until the next thing happens: a wake-up, the end of the work, or the
        //    end of the time slice.
        let mut dt = st.iter().map(|s| s.wake_at).fold(duration_ms, f64::min) - t;
        let mut slice_ends = false;
        if let Some(c) = current {
            dt = dt.min(st[c].work_left);
            if slicing.enabled && !threads[c].is_cooperative() {
                let left = slice_start + slicing.slice_ms.max(0.1) - t;
                if left <= dt + EPS {
                    dt = left.max(0.0);
                    slice_ends = true;
                }
            }
        }
        let end = t + dt;
        let who = current.map_or(0, |c| c + 1);
        match trace.running.last_mut() {
            Some(s) if s.who == who && (s.end_ms - t).abs() < EPS => s.end_ms = end,
            _ => trace.running.push(Span { who, start_ms: t, end_ms: end }),
        }
        for i in (0..n).filter(|&i| st[i].ready && current != Some(i)) {
            match last_ready[i].map(|k| &mut trace.ready[k]) {
                Some(s) if (s.end_ms - t).abs() < EPS => s.end_ms = end,
                _ => {
                    trace.ready.push(Span { who: i + 1, start_ms: t, end_ms: end });
                    last_ready[i] = Some(trace.ready.len() - 1);
                }
            }
        }
        match current {
            None => idle += dt,
            Some(c) => {
                trace.stats[c].cpu_ms += dt;
                st[c].work_left -= dt;
                if st[c].work_left <= EPS {
                    st[c].work_left = threads[c].work_ms.max(0.05);
                    if threads[c].sleep_ms > 0.0 {
                        // k_sleep(): unready until the time is up.
                        st[c].ready = false;
                        st[c].wake_at = end + threads[c].sleep_ms;
                        current = None;
                        slice_ends = false;
                    }
                }
                if slice_ends {
                    // The implicit k_yield(): to the back of the queue.
                    st[c].seq = next_seq;
                    next_seq += 1;
                    slice_start = end;
                    if (0..n).any(|i| i != c && st[i].ready && threads[i].priority == threads[c].priority) {
                        trace.stats[c].sliced += 1;
                        yielded = true;
                    }
                }
            }
        }
        t = end;
    }
    for (s, stats) in st.iter().zip(&mut trace.stats) {
        if let Some(since) = s.ready_since {
            stats.worst_wait_ms = stats.worst_wait_ms.max(duration_ms - since);
            stats.waiting_at_end = true;
        }
    }
    trace.idle_share = if duration_ms > 0.0 { idle / duration_ms } else { 1.0 };
    trace
}

// ---------------------------------------------------------------------------------
// Devicetree: one LED described for the firmware
// ---------------------------------------------------------------------------------

/// Where an LED is, either in reality (how the board is wired) or as a devicetree
/// describes it (`gpios = <&gpio0 13 GPIO_ACTIVE_LOW>;`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedPin {
    /// GPIO controller: 0 = `gpio0`, 1 = `gpio1`.
    pub port: u8,
    pub pin: u8,
    /// True: the LED lights when the pin is low.
    pub active_low: bool,
}

impl LedPin {
    /// The devicetree source describing this LED, in the style of Zephyr's board files.
    pub fn dts(&self) -> String {
        format!(
            "/ {{\n    aliases {{\n        led0 = &led0;\n    }};\n\n    leds {{\n        compatible = \"gpio-leds\";\n        led0: led_0 {{\n            gpios = <&gpio{} {} {}>;\n        }};\n    }};\n}};",
            self.port,
            self.pin,
            if self.active_low { "GPIO_ACTIVE_LOW" } else { "GPIO_ACTIVE_HIGH" }
        )
    }

    /// The electrical level `gpio_pin_set_dt(&led, value)` puts on the pin: the logical
    /// value, inverted if the devicetree marks the pin active-low.
    pub fn level_for(&self, logical_on: bool) -> bool {
        logical_on != self.active_low
    }
}

/// Does the real LED light up? `described` is what the devicetree says, `wired` is the
/// truth on the board. The firmware drives the described pin; the LED only reacts if
/// that is the pin it is really connected to.
pub fn led_lit(described: LedPin, wired: LedPin, logical_on: bool) -> bool {
    let same_pin = (described.port, described.pin) == (wired.port, wired.pin);
    same_pin && described.level_for(logical_on) != wired.active_low
}

// ---------------------------------------------------------------------------------
// Kconfig: which features are built in
// ---------------------------------------------------------------------------------

/// A bool configuration symbol, reduced to the three mechanisms that matter first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Symbol {
    pub name: &'static str,
    /// What the symbol switches on. `None`: an invisible symbol, which "users have no
    /// direct control over".
    pub prompt: Option<&'static str>,
    /// `depends on`: all of these must be enabled.
    pub depends_on: &'static [&'static str],
    /// `select`: these are forced on when this symbol is enabled.
    pub selects: &'static [&'static str],
    /// `default y`.
    pub default_on: bool,
    /// Flash the feature costs, in KB. Illustrative.
    pub flash_kb: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Set in prj.conf and respected.
    Assigned,
    /// Not mentioned in prj.conf: the default applies.
    Default,
    /// Forced on by another symbol's `select`.
    Selected(&'static str),
    /// Off because a dependency is off (whatever prj.conf says).
    DependencyOff(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Final value and the reason, in the order of the symbol list.
    pub values: Vec<(bool, Why)>,
    pub warnings: Vec<String>,
    pub flash_kb: f64,
}

/// Work out the final configuration from the symbol definitions and the assignments in
/// prj.conf. "Assignments in configuration files are only respected if the dependencies
/// for the symbol are satisfied. A warning is printed otherwise."
pub fn kconfig(symbols: &[Symbol], prj_conf: &[(&str, bool)]) -> Resolved {
    let index = |name: &str| symbols.iter().position(|s| s.name == name);
    let mut on = vec![false; symbols.len()];
    let mut why = vec![Why::Default; symbols.len()];
    // Repeat until nothing changes: a select or a dependency can ripple through.
    for _ in 0..symbols.len() + 2 {
        for (i, s) in symbols.iter().enumerate() {
            let selector = symbols.iter().enumerate().find(|(j, o)| on[*j] && o.selects.contains(&s.name)).map(|(_, o)| o.name);
            let missing = s.depends_on.iter().copied().find(|d| !index(d).is_some_and(|j| on[j]));
            let wish = if s.prompt.is_some() { prj_conf.iter().find(|(n, _)| *n == s.name).map(|(_, v)| *v) } else { None };
            (on[i], why[i]) = match (selector, missing, wish) {
                (Some(by), _, _) => (true, Why::Selected(by)),
                (None, Some(dep), _) => (false, Why::DependencyOff(dep)),
                (None, None, Some(v)) => (v, Why::Assigned),
                (None, None, None) => (s.default_on, Why::Default),
            };
        }
    }
    let mut warnings = Vec::new();
    for (name, wish) in prj_conf {
        match index(name) {
            None => warnings.push(format!("CONFIG_{name} is not a known symbol")),
            Some(i) if symbols[i].prompt.is_none() => warnings.push(format!("CONFIG_{name} has no prompt: it cannot be set in prj.conf, only selected by other symbols")),
            Some(i) if on[i] != *wish => warnings.push(match why[i] {
                Why::DependencyOff(dep) => format!("CONFIG_{name} was assigned 'y' but got 'n': it depends on CONFIG_{dep}, which is off"),
                Why::Selected(by) => format!("CONFIG_{name} was assigned 'n' but got 'y': CONFIG_{by} selects it"),
                _ => format!("CONFIG_{name} did not get the assigned value"),
            }),
            _ => {}
        }
    }
    let flash_kb = symbols.iter().zip(&on).filter(|(_, on)| **on).map(|(s, _)| s.flash_kb).sum();
    Resolved { values: on.into_iter().zip(why).collect(), warnings, flash_kb }
}

// ---------------------------------------------------------------------------------
// Semaphore: an interrupt tells a thread that there is work
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemScenario {
    /// "A limit that indicates the maximum value the semaphore's count can reach."
    pub limit: u32,
    /// Events (interrupts) arrive in bursts: this many ...
    pub burst: u32,
    /// ... this far apart ...
    pub spacing_ms: f64,
    /// ... and a new burst starts this often.
    pub burst_period_ms: f64,
    /// How long the thread works on one event.
    pub work_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemTrace {
    /// `(time, lost?)` for every event. Lost: the give had no effect.
    pub events: Vec<(f64, bool)>,
    /// `[time, count]` corner points of the semaphore's count.
    pub count: Vec<[f64; 2]>,
    /// `(from, to)`: the thread is working on an event. In between it waits in `k_sem_take`.
    pub working: Vec<(f64, f64)>,
    pub handled: u32,
    pub lost: u32,
    /// Longest time from an event to the thread starting on it.
    pub worst_delay_ms: f64,
}

/// An interrupt handler calls `k_sem_give` for every event; one thread loops over
/// `k_sem_take(K_FOREVER)` and then works on the event. Rules, from the documentation:
/// "Giving the semaphore increments its count, unless the count is already equal to the
/// limit." "Taking the semaphore decrements its count, unless the semaphore is
/// unavailable (i.e. at zero)", in which case the thread waits; "when the semaphore is
/// given, it is taken by the highest priority thread that has waited longest."
pub fn simulate_semaphore(s: &SemScenario, duration_ms: f64) -> SemTrace {
    let mut tr = SemTrace { count: vec![[0.0, 0.0]], ..Default::default() };
    let mut times = Vec::new();
    let mut start = 5.0;
    while start < duration_ms {
        times.extend((0..s.burst).map(|k| start + k as f64 * s.spacing_ms.max(0.01)).filter(|t| *t < duration_ms));
        start += s.burst_period_ms.max(1.0);
    }
    times.sort_by(f64::total_cmp);
    let mut count = 0u32;
    // Times of the gives that are still waiting to be taken (oldest first).
    let mut pending: std::collections::VecDeque<f64> = Default::default();
    let mut busy_until = 0.0f64;
    let take = |tr: &mut SemTrace, at: f64, given_at: f64, busy_until: &mut f64| {
        tr.working.push((at, at + s.work_ms.max(0.01)));
        tr.handled += 1;
        tr.worst_delay_ms = tr.worst_delay_ms.max(at - given_at);
        *busy_until = at + s.work_ms.max(0.01);
    };
    let mut events = times.into_iter().peekable();
    loop {
        let next_event = events.peek().copied().unwrap_or(f64::INFINITY);
        if count > 0 && busy_until <= next_event && busy_until < duration_ms {
            // The thread comes back to k_sem_take and finds the count above zero.
            let given_at = pending.pop_front().unwrap_or(busy_until);
            tr.count.extend([[busy_until, count as f64], [busy_until, count as f64 - 1.0]]);
            count -= 1;
            let at = busy_until;
            take(&mut tr, at, given_at, &mut busy_until);
        } else if let Some(t) = events.next() {
            if count == 0 && busy_until <= t {
                // The thread is waiting: the give wakes it at once, the count stays 0.
                tr.events.push((t, false));
                take(&mut tr, t, t, &mut busy_until);
            } else if count < s.limit {
                tr.events.push((t, false));
                tr.count.extend([[t, count as f64], [t, count as f64 + 1.0]]);
                count += 1;
                pending.push_back(t);
            } else {
                tr.events.push((t, true));
                tr.lost += 1;
            }
        } else {
            break;
        }
    }
    tr.count.push([duration_ms, count as f64]);
    tr
}

// ---------------------------------------------------------------------------------
// Message queue: passing data items from a producer to a consumer
// ---------------------------------------------------------------------------------

/// What the producer does when the queue is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhenFull {
    /// `k_msgq_put(.., K_NO_WAIT)` fails and the producer throws the new item away.
    DropNew,
    /// `k_msgq_put(.., K_FOREVER)`: "the sending thread may choose to wait for space".
    Wait,
    /// `k_msgq_purge()`, then put: "throws away all existing data so the newer data can
    /// be saved" (the documentation's producer example).
    PurgeOld,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MsgqScenario {
    /// "A maximum quantity of data items that can be queued in the ring buffer."
    pub capacity: usize,
    pub when_full: WhenFull,
    /// The producer makes items in bursts: this many, this far apart, a burst this often.
    pub burst: u32,
    pub spacing_ms: f64,
    pub burst_period_ms: f64,
    /// How long the consumer works on one item.
    pub work_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MsgqTrace {
    /// `(time the item was made, item number, fate)`: 0 = queued, 1 = thrown away, 2 = had to wait.
    pub made: Vec<(f64, u32, u8)>,
    /// `(item number, from, to)`: the consumer working on an item.
    pub working: Vec<(u32, f64, f64)>,
    /// `(from, to)`: the producer blocked in `k_msgq_put`.
    pub blocked: Vec<(f64, f64)>,
    /// `[time, items in the queue]` corner points.
    pub fill: Vec<[f64; 2]>,
    /// Item numbers in the queue at the inspected moment, oldest first.
    pub snapshot: Vec<u32>,
    pub delivered: u32,
    pub discarded: u32,
    /// Longest time from an item being made to the consumer starting on it.
    pub worst_latency_ms: f64,
}

/// A producer puts numbered items into a `k_msgq`; one consumer loops over
/// `k_msgq_get(K_FOREVER)`. Simulated in steps of 0.1 ms.
pub fn simulate_msgq(s: &MsgqScenario, duration_ms: f64, inspect_ms: f64) -> MsgqTrace {
    const TICK: f64 = 0.1;
    let ticks = |ms: f64| (ms / TICK).round().max(1.0) as i64;
    let mut tr = MsgqTrace { fill: vec![[0.0, 0.0]], ..Default::default() };
    let mut schedule: Vec<i64> = Vec::new();
    let mut start = 5.0;
    while start < duration_ms {
        schedule.extend((0..s.burst).map(|k| ticks(start + k as f64 * s.spacing_ms)));
        start += s.burst_period_ms.max(1.0);
    }
    schedule.sort();
    let mut next = 0usize;
    // (item number, tick it was made).
    let mut queue: std::collections::VecDeque<(u32, i64)> = Default::default();
    let mut waiting: std::collections::VecDeque<(u32, i64)> = Default::default();
    let mut busy_left = 0i64;
    for k in 0..ticks(duration_ms) {
        let t = k as f64 * TICK;
        let before = queue.len();
        // Producer.
        while next < schedule.len() && schedule[next] <= k {
            let item = (next as u32 + 1, k);
            next += 1;
            let fate = if !waiting.is_empty() {
                waiting.push_back(item);
                2
            } else if queue.len() < s.capacity.max(1) {
                queue.push_back(item);
                0
            } else {
                match s.when_full {
                    WhenFull::DropNew => {
                        tr.discarded += 1;
                        1
                    }
                    WhenFull::Wait => {
                        waiting.push_back(item);
                        2
                    }
                    WhenFull::PurgeOld => {
                        tr.discarded += queue.len() as u32;
                        queue.clear();
                        queue.push_back(item);
                        0
                    }
                }
            };
            tr.made.push((t, item.0, fate));
        }
        // Consumer.
        busy_left = (busy_left - 1).max(0);
        if busy_left == 0 {
            if let Some((id, made)) = queue.pop_front() {
                busy_left = ticks(s.work_ms);
                tr.working.push((id, t, t + busy_left as f64 * TICK));
                tr.delivered += 1;
                tr.worst_latency_ms = tr.worst_latency_ms.max((k - made) as f64 * TICK);
            }
        }
        // A blocked producer gets the space that has become free.
        while queue.len() < s.capacity.max(1) {
            match waiting.pop_front() {
                Some(item) => queue.push_back(item),
                None => break,
            }
        }
        if !waiting.is_empty() {
            match tr.blocked.last_mut() {
                Some(b) if (b.1 - t).abs() < 1e-6 => b.1 = t + TICK,
                _ => tr.blocked.push((t, t + TICK)),
            }
        }
        if queue.len() != before {
            tr.fill.extend([[t, before as f64], [t, queue.len() as f64]]);
        }
        if k == ticks(inspect_ms) {
            tr.snapshot = queue.iter().map(|q| q.0).collect();
        }
    }
    tr.fill.push([duration_ms, queue.len() as f64]);
    tr
}

// ---------------------------------------------------------------------------------
// Workqueue: one shared thread runs handlers that interrupts hand to it
// ---------------------------------------------------------------------------------

/// A work item that an interrupt submits every `period_ms`; its handler runs for
/// `handler_ms` on the workqueue thread.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkItem {
    pub name: String,
    pub period_ms: f64,
    pub offset_ms: f64,
    pub handler_ms: f64,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkTrace {
    /// `(item, time, had no effect because the item was already queued?)`.
    pub submits: Vec<(usize, f64, bool)>,
    /// `(item, from, to)`: the workqueue thread running a handler.
    pub running: Vec<(usize, f64, f64)>,
    /// Per item: how often its handler ran, how many submissions had no effect, and the
    /// longest time from a submission to the handler starting.
    pub runs: Vec<u32>,
    pub merged: Vec<u32>,
    pub worst_delay_ms: Vec<f64>,
    /// Share of the time the workqueue thread was busy, 0..=1.
    pub busy_share: f64,
}

/// "A workqueue is a kernel object that uses a dedicated thread to process work items in
/// a first in, first out manner." "If an ISR or a thread attempts to submit a work item
/// that is already queued the work item is not affected" - "the work is only performed
/// once." Simulated in steps of 0.1 ms.
pub fn simulate_workqueue(items: &[WorkItem], duration_ms: f64) -> WorkTrace {
    const TICK: f64 = 0.1;
    let ticks = |ms: f64| (ms / TICK).round().max(1.0) as i64;
    let n = items.len();
    let mut tr = WorkTrace { runs: vec![0; n], merged: vec![0; n], worst_delay_ms: vec![0.0; n], ..Default::default() };
    // Queued items, oldest first, with the tick of the submission that queued them.
    let mut queue: std::collections::VecDeque<(usize, i64)> = Default::default();
    let mut busy_left = 0i64;
    let mut busy = 0i64;
    let total = ticks(duration_ms);
    for k in 0..total {
        let t = k as f64 * TICK;
        for (i, item) in items.iter().enumerate().filter(|(_, it)| it.enabled) {
            let since = k - (item.offset_ms / TICK).round() as i64;
            if since >= 0 && since % ticks(item.period_ms) == 0 {
                let already = queue.iter().any(|q| q.0 == i);
                tr.submits.push((i, t, already));
                if already {
                    tr.merged[i] += 1;
                } else {
                    queue.push_back((i, k));
                }
            }
        }
        busy_left = (busy_left - 1).max(0);
        if busy_left == 0 {
            if let Some((i, submitted)) = queue.pop_front() {
                busy_left = ticks(items[i].handler_ms);
                tr.running.push((i, t, t + busy_left as f64 * TICK));
                tr.runs[i] += 1;
                tr.worst_delay_ms[i] = tr.worst_delay_ms[i].max((k - submitted) as f64 * TICK);
            }
        }
        if busy_left > 0 {
            busy += 1;
        }
    }
    tr.busy_share = busy as f64 / total.max(1) as f64;
    tr
}

// ---------------------------------------------------------------------------------
// From an interrupt to the thread that deals with it
// ---------------------------------------------------------------------------------

/// A busy background thread is running when an interrupt arrives. The interrupt
/// handler (ISR) gives a semaphore that a more urgent thread is waiting for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsrScenario {
    /// The moment the hardware raises the interrupt.
    pub irq_at_ms: f64,
    /// How long the ISR itself runs.
    pub isr_ms: f64,
    /// How long the urgent thread works once it has the processor.
    pub urgent_work_ms: f64,
    /// The background thread has a negative priority: no thread can preempt it.
    pub background_cooperative: bool,
    /// It gives up the processor (`k_yield`) every so often.
    pub background_yields_every_ms: f64,
    /// It holds an IRQ lock from `lock_at_ms` for `lock_ms` (0 = never).
    pub lock_at_ms: f64,
    pub lock_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsrTrace {
    pub isr: (f64, f64),
    pub urgent: (f64, f64),
    /// Delay from the interrupt to the first instruction of the ISR.
    pub isr_latency_ms: f64,
    /// Delay from the interrupt to the urgent thread running.
    pub thread_latency_ms: f64,
}

/// Rules, from the documentation: "A thread may temporarily prevent all IRQ handling in
/// the system using an IRQ lock." "An ISR normally preempts the execution of the current
/// thread" - cooperative or not. "Thread execution resumes only once all ISR work has
/// been completed", and then the scheduler decides: a preemptible thread gives way to
/// the more urgent one at once, a cooperative thread "remains the current thread until
/// it performs an action that makes it unready" - here, until its next `k_yield`.
pub fn interrupt_to_thread(s: &IsrScenario) -> IsrTrace {
    let locked = s.lock_ms > 0.0 && s.irq_at_ms >= s.lock_at_ms && s.irq_at_ms < s.lock_at_ms + s.lock_ms;
    let isr_start = if locked { s.lock_at_ms + s.lock_ms } else { s.irq_at_ms };
    let isr_end = isr_start + s.isr_ms;
    let urgent_start = if s.background_cooperative {
        // The background thread's own running time stops while the ISR runs, so its
        // yield points after the ISR are pushed back by the ISR's length.
        let every = s.background_yields_every_ms.max(0.01);
        let ran_before_isr = isr_start;
        let next_yield_after = (ran_before_isr / every).floor() * every + every;
        next_yield_after + s.isr_ms
    } else {
        isr_end
    };
    IsrTrace { isr: (isr_start, isr_end), urgent: (urgent_start, urgent_start + s.urgent_work_ms), isr_latency_ms: isr_start - s.irq_at_ms, thread_latency_ms: urgent_start - s.irq_at_ms }
}

// ---------------------------------------------------------------------------------
// Ticks, timers, and why a sleep in a loop drifts
// ---------------------------------------------------------------------------------

/// Whole ticks needed to cover `ms`: a timeout is "a minimum delay", so it is rounded up.
pub fn ticks_for(ms: f64, ticks_per_s: f64) -> u64 {
    (ms * ticks_per_s / 1000.0 - 1e-9).ceil().max(0.0) as u64
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeatScenario {
    /// `CONFIG_SYS_CLOCK_TICKS_PER_SEC`.
    pub ticks_per_s: f64,
    /// The wanted time between two beats.
    pub period_ms: f64,
    /// The work done on every beat.
    pub work_ms: f64,
}

impl BeatScenario {
    pub fn tick_ms(&self) -> f64 {
        1000.0 / self.ticks_per_s
    }

    /// The period a kernel timer really gets: the wanted one rounded up to whole ticks.
    pub fn timer_period_ms(&self) -> f64 {
        ticks_for(self.period_ms, self.ticks_per_s).max(1) as f64 * self.tick_ms()
    }

    /// The period of a thread that works and then calls `k_sleep(period)`: the work
    /// ends somewhere inside a tick, the sleep starts counting at the next tick edge.
    pub fn sleep_loop_period_ms(&self) -> f64 {
        ticks_for(self.work_ms, self.ticks_per_s) as f64 * self.tick_ms() + self.timer_period_ms()
    }

    /// Start times of the first beats: `(periodic k_timer, work-then-sleep loop)`.
    pub fn beats(&self, until_ms: f64) -> (Vec<f64>, Vec<f64>) {
        let series = |period: f64| (0..10_000).map(|k| k as f64 * period).take_while(|t| *t < until_ms).collect();
        (series(self.timer_period_ms()), series(self.sleep_loop_period_ms()))
    }
}

// ---------------------------------------------------------------------------------
// Boot: the order in which drivers and init functions run before main()
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InitLevel {
    /// Devices with no dependencies. "Kernel services are unavailable."
    PreKernel1,
    /// Devices that depend on PRE_KERNEL_1 devices. Still no kernel services.
    PreKernel2,
    /// Devices that need kernel services during configuration.
    PostKernel,
    /// Init functions of the application, just before main().
    Application,
}

impl InitLevel {
    pub const ALL: [InitLevel; 4] = [InitLevel::PreKernel1, InitLevel::PreKernel2, InitLevel::PostKernel, InitLevel::Application];

    pub fn name(self) -> &'static str {
        match self {
            InitLevel::PreKernel1 => "PRE_KERNEL_1",
            InitLevel::PreKernel2 => "PRE_KERNEL_2",
            InitLevel::PostKernel => "POST_KERNEL",
            InitLevel::Application => "APPLICATION",
        }
    }

    pub fn kernel_available(self) -> bool {
        self >= InitLevel::PostKernel
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InitItem {
    pub name: String,
    pub level: InitLevel,
    /// Lower values initialize earlier within a level.
    pub priority: u16,
    /// Index of the item that must be up before this one.
    pub depends_on: Option<usize>,
    /// Its init function sleeps, waits or takes a lock.
    pub needs_kernel: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootProblem {
    /// What it depends on has not been initialized yet (or failed itself).
    DependencyNotReady(usize),
    /// It uses kernel services in a level where they do not exist yet.
    NoKernelYet,
}

/// The order in which the items are initialized - by level, then by priority number,
/// then as listed - and what goes wrong for each: `(item, problem)`.
pub fn boot_order(items: &[InitItem]) -> Vec<(usize, Option<BootProblem>)> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| (items[i].level, items[i].priority, i));
    let mut ready = vec![false; items.len()];
    order
        .into_iter()
        .map(|i| {
            let problem = if items[i].needs_kernel && !items[i].level.kernel_available() {
                Some(BootProblem::NoKernelYet)
            } else {
                items[i].depends_on.filter(|&d| !ready[d]).map(BootProblem::DependencyNotReady)
            };
            ready[i] = problem.is_none();
            (i, problem)
        })
        .collect()
}

// ---------------------------------------------------------------------------------
// Power management: which sleep state the kernel picks when nothing is ready to run
// ---------------------------------------------------------------------------------

/// A low-power state as a devicetree `zephyr,power-state` node describes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerState {
    pub name: &'static str,
    pub current_a: f64,
    /// `min-residency-us`: staying in the state for less than this is not worth it.
    pub min_residency_us: f64,
    /// `exit-latency-us`: how long waking up from it takes.
    pub exit_latency_us: f64,
}

/// The residency policy. States are listed from lightest to deepest; the deepest one
/// that fits and is not locked wins: `if (time_to_next_scheduled_event >=
/// (state.min_residency_us + state.exit_latency))`. `None`: stay awake (plain idle).
pub fn pick_power_state(states: &[PowerState], idle_us: f64, locked: &[bool]) -> Option<usize> {
    (0..states.len()).rev().find(|&i| !locked.get(i).copied().unwrap_or(false) && idle_us >= states[i].min_residency_us + states[i].exit_latency_us)
}

/// Average current of a system that works for `work_us` at `run_a` and is then idle
/// for `idle_us` until the next event, sleeping in the state the policy picks. Waking
/// up (the exit latency) is counted at the run current.
pub fn pm_average_current(states: &[PowerState], run_a: f64, idle_a: f64, work_us: f64, idle_us: f64, locked: &[bool]) -> f64 {
    let charge = match pick_power_state(states, idle_us, locked) {
        Some(i) => states[i].current_a * (idle_us - states[i].exit_latency_us) + run_a * states[i].exit_latency_us,
        None => idle_a * idle_us,
    };
    (run_a * work_us + charge) / (work_us + idle_us)
}

// ---------------------------------------------------------------------------------
// Device drivers: one API, different hardware behind it
// ---------------------------------------------------------------------------------

/// Zephyr's `struct sensor_value`: the integer part and the fractional part in
/// millionths, both with the sign of the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensorValue {
    pub val1: i32,
    pub val2: i32,
}

impl SensorValue {
    pub fn from_f64(x: f64) -> Self {
        let micro = (x * 1e6).round() as i64;
        Self { val1: (micro / 1_000_000) as i32, val2: (micro % 1_000_000) as i32 }
    }
}

/// Two made-up temperature sensors of the kinds found on real boards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TempSensor {
    /// A digital chip on the I2C bus: it returns a signed count of 0.0625 degree steps.
    DigitalI2c,
    /// An analog sensor on an ADC pin: 500 mV at 0 degrees plus 10 mV per degree,
    /// measured with a 12-bit converter and a 3.3 V reference.
    AnalogAdc,
}

/// What a driver's "fetch" function reads from the hardware.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawSample {
    /// The number the hardware delivers.
    pub raw: i32,
    /// The temperature the driver computes from it, degrees Celsius.
    pub celsius: f64,
}

impl TempSensor {
    /// Read the sensor at a true temperature of `celsius`.
    pub fn fetch(self, celsius: f64) -> RawSample {
        match self {
            TempSensor::DigitalI2c => {
                let raw = (celsius / 0.0625).round() as i32;
                RawSample { raw, celsius: raw as f64 * 0.0625 }
            }
            TempSensor::AnalogAdc => {
                let volts = 0.5 + 0.010 * celsius;
                let raw = ((volts / 3.3 * 4096.0).floor() as i32).clamp(0, 4095);
                RawSample { raw, celsius: (raw as f64 * 3.3 / 4096.0 - 0.5) / 0.010 }
            }
        }
    }

    /// Smallest temperature change the sensor can report.
    pub fn step_celsius(self) -> f64 {
        match self {
            TempSensor::DigitalI2c => 0.0625,
            TempSensor::AnalogAdc => 3.3 / 4096.0 / 0.010,
        }
    }
}

// ---------------------------------------------------------------------------------
// Logging: what a log line costs
// ---------------------------------------------------------------------------------

/// Severity levels with Zephyr's numbers: a module compiled at level N keeps the
/// messages with a number up to N.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Err = 1,
    Wrn = 2,
    Inf = 3,
    Dbg = 4,
}

impl LogLevel {
    pub fn tag(self) -> &'static str {
        ["", "err", "wrn", "inf", "dbg"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogCall {
    pub at_ms: f64,
    pub module: &'static str,
    pub level: LogLevel,
    pub text: &'static str,
}

impl LogCall {
    /// The line as Zephyr's default output prints it:
    /// `"[%02u:%02u:%02u.%03u,%03u] <%s> %s: "` followed by the message.
    pub fn line(&self) -> String {
        let us = (self.at_ms * 1000.0).round() as u64;
        let (ms, s) = (us / 1000 % 1000, us / 1_000_000);
        format!("[{:02}:{:02}:{:02}.{:03},{:03}] <{}> {}: {}", s / 3600, s / 60 % 60, s % 60, ms, us % 1000, self.level.tag(), self.module, self.text)
    }
}

/// What became of one log call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogFate {
    /// Its level is above the module's compiled-in level: the call does not exist.
    CompiledOut,
    /// Deferred mode, and the buffer was full: "the new log is dropped".
    Dropped,
    /// Printed on the serial line during `(from, to)`.
    Printed(f64, f64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogRun {
    pub fates: Vec<LogFate>,
    /// Per call: how long the calling thread was held up, milliseconds.
    pub held_ms: Vec<f64>,
    /// Bytes of message text that end up in flash.
    pub text_bytes: usize,
}

/// Run a list of log calls (sorted by time). `levels` gives the compiled-in level per
/// module (0 = none). In immediate mode "the log message is processed by the backends
/// ... in the calling context": the caller waits while the line goes out at `baud`.
/// In deferred mode the caller only stores the message (taken as free here) and a
/// logging thread prints it later; at most `buffer` messages can wait.
pub fn run_logging(calls: &[LogCall], levels: &[(&str, u8)], immediate: bool, baud: f64, buffer: usize) -> LogRun {
    let mut run = LogRun::default();
    let mut line_free_at = 0.0f64;
    // End times of the messages still waiting or being printed (deferred mode).
    let mut pending: std::collections::VecDeque<f64> = Default::default();
    // In immediate mode a slow line pushes all later calls of the thread back.
    let mut delay = 0.0;
    for c in calls {
        let level = levels.iter().find(|(m, _)| *m == c.module).map_or(0, |(_, l)| *l);
        if c.level as u8 > level {
            run.fates.push(LogFate::CompiledOut);
            run.held_ms.push(0.0);
            continue;
        }
        run.text_bytes += c.text.len() + 1;
        let print_ms = c.line().len() as f64 * 10.0 / baud * 1000.0;
        if immediate {
            let start = c.at_ms + delay;
            run.fates.push(LogFate::Printed(start, start + print_ms));
            run.held_ms.push(print_ms);
            delay += print_ms;
        } else {
            while pending.front().is_some_and(|end| *end <= c.at_ms) {
                pending.pop_front();
            }
            if pending.len() >= buffer.max(1) {
                run.fates.push(LogFate::Dropped);
            } else {
                let start = line_free_at.max(c.at_ms);
                line_free_at = start + print_ms;
                pending.push_back(line_free_at);
                run.fates.push(LogFate::Printed(start, line_free_at));
            }
            run.held_ms.push(0.0);
        }
    }
    run
}

// ---------------------------------------------------------------------------------
// Bluetooth: which layers of the stack are in which chip
// ---------------------------------------------------------------------------------

/// The three ways Zephyr's Bluetooth stack can be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BtBuild {
    /// "Application, Host, and Controller in one image", for single-chip designs.
    Combined,
    /// "The Application and the Bluetooth Host", plus an HCI driver for an external controller.
    HostOnly,
    /// "The Link Layer and a special application" that bridges HCI to a wire.
    ControllerOnly,
}

impl BtBuild {
    /// Is this layer part of the firmware you build? Layers from the top:
    /// 0 application, 1 host, 2 controller (link layer), 3 radio hardware.
    pub fn contains(self, layer: usize) -> bool {
        match self {
            BtBuild::Combined => true,
            BtBuild::HostOnly => layer <= 1,
            BtBuild::ControllerOnly => layer >= 2,
        }
    }

    /// The lines of prj.conf that select this build.
    pub fn prj_conf(self) -> &'static str {
        match self {
            BtBuild::Combined | BtBuild::HostOnly => "CONFIG_BT=y\nCONFIG_BT_HCI=y",
            BtBuild::ControllerOnly => "CONFIG_BT=y\nCONFIG_BT_HCI=y\nCONFIG_BT_HCI_RAW=y",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(name: &str, priority: i8, work: f64, sleep: f64) -> Thread {
        Thread { name: name.into(), priority, work_ms: work, sleep_ms: sleep, enabled: true }
    }

    const OFF: Slicing = Slicing { enabled: false, slice_ms: 10.0 };
    const ON: Slicing = Slicing { enabled: true, slice_ms: 10.0 };

    fn spans(trace: &Trace, from: usize, count: usize) -> Vec<(usize, f64, f64)> {
        trace.running.iter().skip(from).take(count).map(|s| (s.who, s.start_ms, s.end_ms)).collect()
    }

    #[test]
    fn among_equals_the_one_that_waited_longest_runs_first() {
        // "h" works 0..6 and 10..16. During its second turn "a" wakes at 11 and "b" at 13.
        // "a" has waited longer, so it runs first although "b" comes first in the list.
        let threads = [thread("h", 0, 6.0, 4.0), thread("b", 3, 1.0, 6.0), thread("a", 3, 1.0, 3.0)];
        let trace = simulate(&threads, OFF, 30.0);
        assert_eq!(spans(&trace, 0, 3), [(1, 0.0, 6.0), (2, 6.0, 7.0), (3, 7.0, 8.0)]);
        let i = trace.running.iter().position(|s| s.who == 1 && s.start_ms == 10.0).unwrap();
        assert_eq!(spans(&trace, i, 3), [(1, 10.0, 16.0), (3, 16.0, 17.0), (2, 17.0, 18.0)]);
        assert!(trace.wakes.contains(&(2, 11.0)) && trace.wakes.contains(&(1, 13.0)));
    }

    #[test]
    fn a_cooperative_thread_is_not_preempted_but_a_preemptible_one_is() {
        // "high" wakes at 5 while "low" is working 1..9.
        let coop = [thread("high", -2, 1.0, 4.0), thread("low", -1, 8.0, 20.0)];
        let trace = simulate(&coop, ON, 100.0);
        assert_eq!(spans(&trace, 0, 3), [(1, 0.0, 1.0), (2, 1.0, 9.0), (1, 9.0, 10.0)]);
        assert_eq!(trace.stats[0].worst_wait_ms, 4.0);
        assert_eq!(trace.stats[1].preempted, 0);

        let preemptible = [thread("high", -2, 1.0, 4.0), thread("low", 0, 8.0, 20.0)];
        let trace = simulate(&preemptible, ON, 100.0);
        assert_eq!(spans(&trace, 0, 4), [(1, 0.0, 1.0), (2, 1.0, 5.0), (1, 5.0, 6.0), (2, 6.0, 10.0)]);
        assert_eq!(trace.stats[0].worst_wait_ms, 0.0);
        assert!(trace.stats[1].preempted > 0);
    }

    #[test]
    fn time_slicing_shares_the_processor_between_equals() {
        let threads = [thread("a", 5, 1.0, 0.0), thread("b", 5, 1.0, 0.0)];
        let trace = simulate(&threads, ON, 100.0);
        assert_eq!(spans(&trace, 0, 3), [(1, 0.0, 10.0), (2, 10.0, 20.0), (1, 20.0, 30.0)]);
        assert_eq!((trace.stats[0].cpu_ms, trace.stats[1].cpu_ms), (50.0, 50.0));
        assert_eq!(trace.stats[0].sliced, 5);
        assert_eq!(trace.stats[0].preempted, 0, "the end of a slice is not a preemption");

        // Without it the first one keeps the processor for ever.
        let trace = simulate(&threads, OFF, 100.0);
        assert_eq!(trace.stats[0].cpu_ms, 100.0);
        assert_eq!(trace.stats[1].runs, 0);
        assert!(trace.stats[1].waiting_at_end);
        assert_eq!(trace.stats[1].worst_wait_ms, 100.0);

        // A thread with no equal keeps running through the end of its slices.
        let trace = simulate(&threads[..1], ON, 100.0);
        assert_eq!(trace.running.len(), 1);
        assert_eq!(trace.stats[0].sliced, 0);

        // Cooperative threads are not sliced.
        let coop = [thread("a", -1, 1.0, 0.0), thread("b", -1, 1.0, 0.0)];
        assert_eq!(simulate(&coop, ON, 100.0).stats[1].runs, 0);
    }

    #[test]
    fn a_new_thread_restarts_the_time_slice() {
        // "h" runs 0..1 and 7..8. "a" is pushed aside at 7 after 6 ms and then gets a
        // whole new 10 ms slice from 8, which again is cut short at 14.
        let threads = [thread("h", 0, 1.0, 6.0), thread("a", 5, 1.0, 0.0), thread("b", 5, 1.0, 0.0)];
        let trace = simulate(&threads, ON, 30.0);
        assert_eq!(spans(&trace, 0, 5), [(1, 0.0, 1.0), (2, 1.0, 7.0), (1, 7.0, 8.0), (2, 8.0, 14.0), (1, 14.0, 15.0)]);
        assert_eq!(trace.stats[1].sliced, 0);
    }

    #[test]
    fn the_demo_scenes_behave_as_the_explanation_says() {
        let scene = |sensor_priority, sensor_work, sensor_sleep| {
            [thread("radio", -2, 1.0, 9.0), thread("sensor", sensor_priority, sensor_work, sensor_sleep), thread("maths a", 5, 1.0, 0.0), thread("maths b", 5, 1.0, 0.0)]
        };
        let slice = |ms| Slicing { enabled: true, slice_ms: ms };
        let cpu = |trace: &Trace| -> Vec<f64> { trace.stats.iter().map(|s| s.cpu_ms).collect() };
        // Opening state, 4 ms slices: everyone runs, the radio never waits, nothing is idle.
        let trace = simulate(&scene(1, 3.0, 17.0), slice(4.0), 100.0);
        assert_eq!(cpu(&trace), [10.0, 15.0, 38.0, 37.0]);
        assert!(trace.stats.iter().all(|s| s.runs > 0 && !s.waiting_at_end));
        assert_eq!(trace.stats[0].worst_wait_ms, 0.0);
        assert_eq!(trace.idle_share, 0.0);
        // Slicing off: "maths b" never runs.
        let trace = simulate(&scene(1, 3.0, 17.0), OFF, 100.0);
        assert_eq!(cpu(&trace), [10.0, 15.0, 75.0, 0.0]);
        // 20 ms slices, but the radio thread interrupts every 10 ms: "maths a" gets a
        // fresh slice each time and "maths b" never runs either.
        let trace = simulate(&scene(1, 3.0, 17.0), slice(20.0), 100.0);
        assert_eq!(cpu(&trace), [10.0, 15.0, 75.0, 0.0]);
        assert_eq!(trace.stats[2].sliced, 0);
        // The sensor thread made cooperative with 8 ms of work: the radio has to wait.
        let trace = simulate(&scene(-1, 8.0, 17.0), slice(4.0), 100.0);
        assert_eq!(trace.stats[0].worst_wait_ms, 5.0);
        // A cooperative thread that never sleeps: nobody else runs again.
        let trace = simulate(&scene(-1, 8.0, 0.0), slice(4.0), 100.0);
        assert_eq!(cpu(&trace), [1.0, 99.0, 0.0, 0.0]);
        assert!(trace.stats[0].waiting_at_end && trace.stats[0].worst_wait_ms == 90.0);
    }

    #[test]
    fn devicetree_led_logic() {
        let board = LedPin { port: 0, pin: 13, active_low: true };
        assert!(board.dts().contains("gpios = <&gpio0 13 GPIO_ACTIVE_LOW>;"));
        // Logical "on" on an active-low pin is electrically low.
        assert!(!board.level_for(true));
        assert!(board.level_for(false));
        // A matching description: on means lit.
        assert!(led_lit(board, board, true) && !led_lit(board, board, false));
        // Wrong polarity in the devicetree: the LED is lit exactly when it should be off.
        let wrong_flag = LedPin { active_low: false, ..board };
        assert!(!led_lit(wrong_flag, board, true) && led_lit(wrong_flag, board, false));
        // Wrong pin: some other pin toggles, the LED stays dark.
        let wrong_pin = LedPin { pin: 14, ..board };
        assert!(!led_lit(wrong_pin, board, true) && !led_lit(wrong_pin, board, false));
    }

    const SYMBOLS: [Symbol; 4] = [
        Symbol { name: "A", prompt: Some("a"), depends_on: &[], selects: &[], default_on: false, flash_kb: 1.0 },
        Symbol { name: "B", prompt: Some("b"), depends_on: &["A"], selects: &["HIDDEN"], default_on: false, flash_kb: 2.0 },
        Symbol { name: "C", prompt: Some("c"), depends_on: &["A"], selects: &[], default_on: true, flash_kb: 4.0 },
        Symbol { name: "HIDDEN", prompt: None, depends_on: &[], selects: &[], default_on: false, flash_kb: 8.0 },
    ];

    #[test]
    fn kconfig_depends_default_and_select() {
        // Nothing set: everything off.
        let r = kconfig(&SYMBOLS, &[]);
        assert!(r.values.iter().all(|v| !v.0));
        assert_eq!(r.values[2].1, Why::DependencyOff("A"));
        // A on: C follows by its default, B stays off.
        let r = kconfig(&SYMBOLS, &[("A", true)]);
        assert_eq!(r.values.iter().map(|v| v.0).collect::<Vec<_>>(), [true, false, true, false]);
        assert_eq!((r.values[0].1, r.values[2].1), (Why::Assigned, Why::Default));
        assert_eq!(r.flash_kb, 5.0);
        // B on as well: it selects the invisible symbol.
        let r = kconfig(&SYMBOLS, &[("A", true), ("B", true), ("C", false)]);
        assert_eq!(r.values.iter().map(|v| v.0).collect::<Vec<_>>(), [true, true, false, true]);
        assert_eq!(r.values[3].1, Why::Selected("B"));
        assert!(r.warnings.is_empty());
        // B without A: ignored, with a warning.
        let r = kconfig(&SYMBOLS, &[("B", true)]);
        assert!(!r.values[1].0 && !r.values[3].0);
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("depends on CONFIG_A"));
        // An invisible symbol cannot be set.
        assert_eq!(kconfig(&SYMBOLS, &[("HIDDEN", true)]).warnings.len(), 1);
    }

    #[test]
    fn a_binary_semaphore_loses_events_a_counting_one_does_not() {
        // Bursts of 3 events 1 ms apart, the thread needs 5 ms per event.
        let s = SemScenario { limit: 1, burst: 3, spacing_ms: 1.0, burst_period_ms: 40.0, work_ms: 5.0 };
        let tr = simulate_semaphore(&s, 40.0);
        // 5 ms: the waiting thread takes it at once. 6 ms: count goes to 1. 7 ms: already at the limit, lost.
        assert_eq!(tr.events, [(5.0, false), (6.0, false), (7.0, true)]);
        assert_eq!(tr.working, [(5.0, 10.0), (10.0, 15.0)]);
        assert_eq!((tr.handled, tr.lost), (2, 1));
        assert_eq!(tr.worst_delay_ms, 4.0);
        // Limit 3: the count climbs to 2 and every event is handled, the last one 8 ms late.
        let tr = simulate_semaphore(&SemScenario { limit: 3, ..s }, 40.0);
        assert_eq!((tr.handled, tr.lost), (3, 0));
        assert_eq!(tr.working, [(5.0, 10.0), (10.0, 15.0), (15.0, 20.0)]);
        assert_eq!(tr.worst_delay_ms, 8.0);
        assert!(tr.count.contains(&[7.0, 2.0]));
    }

    #[test]
    fn message_queue_full_policies() {
        // 4 items 1 ms apart from 5 ms, room for 2, the consumer needs 5 ms per item.
        let s = MsgqScenario { capacity: 2, when_full: WhenFull::DropNew, burst: 4, spacing_ms: 1.0, burst_period_ms: 100.0, work_ms: 5.0 };
        let close = |a: &[(u32, f64, f64)], b: &[(u32, f64, f64)]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 1e-6 && (x.2 - y.2).abs() < 1e-6);
        // Item 1 goes straight to the waiting consumer, 2 and 3 fill the queue, 4 finds it full.
        let tr = simulate_msgq(&s, 60.0, 9.0);
        assert!(close(&tr.working, &[(1, 5.0, 10.0), (2, 10.0, 15.0), (3, 15.0, 20.0)]), "{:?}", tr.working);
        assert_eq!((tr.delivered, tr.discarded), (3, 1));
        assert_eq!(tr.snapshot, [2, 3]);
        assert_eq!(tr.made.iter().map(|m| m.2).collect::<Vec<_>>(), [0, 0, 0, 1]);
        // Purge: item 4 throws 2 and 3 away and is handled next.
        let tr = simulate_msgq(&MsgqScenario { when_full: WhenFull::PurgeOld, ..s }, 60.0, 9.0);
        assert!(close(&tr.working, &[(1, 5.0, 10.0), (4, 10.0, 15.0)]), "{:?}", tr.working);
        assert_eq!((tr.delivered, tr.discarded), (2, 2));
        assert_eq!(tr.snapshot, [4]);
        // Wait: the producer is blocked from 8 to 10 ms, nothing is lost.
        let tr = simulate_msgq(&MsgqScenario { when_full: WhenFull::Wait, ..s }, 60.0, 9.0);
        assert!(close(&tr.working, &[(1, 5.0, 10.0), (2, 10.0, 15.0), (3, 15.0, 20.0), (4, 20.0, 25.0)]), "{:?}", tr.working);
        assert_eq!((tr.delivered, tr.discarded), (4, 0));
        assert_eq!(tr.blocked.len(), 1);
        assert!((tr.blocked[0].0 - 8.0).abs() < 1e-6 && (tr.blocked[0].1 - 10.0).abs() < 0.11, "{:?}", tr.blocked);
        assert!((tr.worst_latency_ms - 12.0).abs() < 1e-6);
    }

    #[test]
    fn workqueue_is_first_in_first_out_and_merges_repeats() {
        let item = |name: &str, period, offset, handler| WorkItem { name: name.into(), period_ms: period, offset_ms: offset, handler_ms: handler, enabled: true };
        // "slow" is submitted at 0 and runs 0..20. "quick" is submitted at 5, 10, 15, 20 ...
        let items = [item("slow", 100.0, 0.0, 20.0), item("quick", 5.0, 5.0, 1.0)];
        let tr = simulate_workqueue(&items, 40.0);
        // The submissions at 10, 15 and 20 find "quick" already queued: no effect.
        assert_eq!(tr.submits.iter().filter(|s| s.0 == 1).map(|s| s.2).collect::<Vec<_>>(), [false, true, true, true, false, false, false]);
        assert_eq!(tr.merged, [0, 3]);
        // It runs once at 20 for the four submissions, 15 ms after the first of them.
        assert_eq!(tr.running[0].0, 0);
        assert!((tr.running[1].1 - 20.0).abs() < 1e-6 && tr.running[1].0 == 1, "{:?}", tr.running);
        assert!((tr.worst_delay_ms[1] - 15.0).abs() < 1e-6);
        assert_eq!(tr.runs, [1, 4]);
    }

    #[test]
    fn interrupt_latencies() {
        let s = IsrScenario { irq_at_ms: 12.0, isr_ms: 0.5, urgent_work_ms: 2.0, background_cooperative: false, background_yields_every_ms: 10.0, lock_at_ms: 0.0, lock_ms: 0.0 };
        // Preemptible background thread: ISR at once, the urgent thread right after it.
        let tr = interrupt_to_thread(&s);
        assert_eq!((tr.isr, tr.urgent), ((12.0, 12.5), (12.5, 14.5)));
        assert_eq!((tr.isr_latency_ms, tr.thread_latency_ms), (0.0, 0.5));
        // Cooperative: the ISR still runs at once, but the thread waits for the yield at
        // 20 ms of the background thread's running time, which the ISR pushed to 20.5.
        let tr = interrupt_to_thread(&IsrScenario { background_cooperative: true, ..s });
        assert_eq!(tr.isr, (12.0, 12.5));
        assert_eq!(tr.urgent.0, 20.5);
        assert_eq!(tr.thread_latency_ms, 8.5);
        // An IRQ lock from 11 to 14 ms holds the ISR itself back by 2 ms.
        let tr = interrupt_to_thread(&IsrScenario { lock_at_ms: 11.0, lock_ms: 3.0, ..s });
        assert_eq!((tr.isr, tr.isr_latency_ms, tr.thread_latency_ms), ((14.0, 14.5), 2.0, 2.5));
        // A lock that is over before the interrupt changes nothing.
        assert_eq!(interrupt_to_thread(&IsrScenario { lock_at_ms: 5.0, lock_ms: 3.0, ..s }).isr_latency_ms, 0.0);
    }

    #[test]
    fn ticks_round_up_and_sleep_loops_drift() {
        assert_eq!(ticks_for(10.0, 100.0), 1);
        assert_eq!(ticks_for(25.0, 100.0), 3);
        assert_eq!(ticks_for(2.5, 1000.0), 3);
        assert_eq!(ticks_for(2.5, 10_000.0), 25);
        // 1 kHz tick, a beat every 10 ms, 2.5 ms of work.
        let b = BeatScenario { ticks_per_s: 1000.0, period_ms: 10.0, work_ms: 2.5 };
        assert_eq!(b.timer_period_ms(), 10.0);
        // Work (rounded up to 3 ticks) plus the 10 ms sleep: 13 ms per round.
        assert_eq!(b.sleep_loop_period_ms(), 13.0);
        let (timer, sleeper) = b.beats(100.0);
        assert_eq!((timer.len(), sleeper.len()), (10, 8));
        assert_eq!(sleeper[3], 39.0);
        // 100 Hz tick: a 25 ms period becomes 30 ms.
        assert_eq!(BeatScenario { ticks_per_s: 100.0, period_ms: 25.0, work_ms: 2.5 }.timer_period_ms(), 30.0);
    }

    #[test]
    fn boot_order_and_its_problems() {
        let item = |name: &str, level, priority, depends_on, needs_kernel| InitItem { name: name.into(), level, priority, depends_on, needs_kernel };
        let items = vec![
            item("clock", InitLevel::PreKernel1, 30, None, false),
            item("sensor", InitLevel::PostKernel, 90, Some(2), true),
            item("i2c", InitLevel::PostKernel, 50, Some(0), false),
        ];
        // Level first, then priority: clock, i2c, sensor. No problems.
        assert_eq!(boot_order(&items), [(0, None), (2, None), (1, None)]);
        // The sensor before its bus: the bus is not ready.
        let mut early = items.clone();
        early[1].priority = 40;
        assert_eq!(boot_order(&early), [(0, None), (1, Some(BootProblem::DependencyNotReady(2))), (2, None)]);
        // The sensor's init function sleeps: not possible before the kernel is up.
        let mut pre = items.clone();
        pre[1].level = InitLevel::PreKernel2;
        assert_eq!(boot_order(&pre)[1], (1, Some(BootProblem::NoKernelYet)));
        // A failed dependency takes its dependants with it.
        let mut chain = items.clone();
        chain[2].level = InitLevel::PreKernel1;
        chain[2].priority = 10;
        assert_eq!(boot_order(&chain), [(2, Some(BootProblem::DependencyNotReady(0))), (0, None), (1, Some(BootProblem::DependencyNotReady(2)))]);
    }

    #[test]
    fn residency_policy_picks_the_deepest_state_that_fits() {
        let states = [
            PowerState { name: "suspend-to-idle", current_a: 800e-6, min_residency_us: 1_000.0, exit_latency_us: 10.0 },
            PowerState { name: "standby", current_a: 30e-6, min_residency_us: 10_000.0, exit_latency_us: 500.0 },
            PowerState { name: "suspend-to-ram", current_a: 2e-6, min_residency_us: 100_000.0, exit_latency_us: 5_000.0 },
        ];
        let none = [false; 3];
        assert_eq!(pick_power_state(&states, 500.0, &none), None);
        assert_eq!(pick_power_state(&states, 1_010.0, &none), Some(0));
        assert_eq!(pick_power_state(&states, 10_499.0, &none), Some(0));
        assert_eq!(pick_power_state(&states, 10_500.0, &none), Some(1));
        assert_eq!(pick_power_state(&states, 1e6, &none), Some(2));
        // The deepest state locked: fall back to the next one.
        assert_eq!(pick_power_state(&states, 1e6, &[false, false, true]), Some(1));
        // 1 ms of work at 4 mA, then 1 s idle in suspend-to-ram (5 ms of it waking up at 4 mA):
        // (4e-3 x 1000 + 2e-6 x 995000 + 4e-3 x 5000) / 1001000 = 25.96 µA.
        let avg = pm_average_current(&states, 4e-3, 1.5e-3, 1_000.0, 1e6, &none);
        assert!((avg - 25.964e-6).abs() < 1e-9, "{avg}");
    }

    #[test]
    fn sensor_drivers_and_sensor_value() {
        assert_eq!(SensorValue::from_f64(25.0625), SensorValue { val1: 25, val2: 62_500 });
        assert_eq!(SensorValue::from_f64(-0.5), SensorValue { val1: 0, val2: -500_000 });
        assert_eq!(SensorValue::from_f64(-3.25), SensorValue { val1: -3, val2: -250_000 });
        // Digital: 25.06 degrees is 401 steps of 0.0625.
        let d = TempSensor::DigitalI2c.fetch(25.06);
        assert_eq!((d.raw, d.celsius), (401, 25.0625));
        assert_eq!(TempSensor::DigitalI2c.fetch(-10.0).raw, -160);
        // Analog: 25 degrees is 750 mV; 0.75 / 3.3 x 4096 = 930.9 -> 930 -> 749.27 mV -> 24.93 degrees.
        let a = TempSensor::AnalogAdc.fetch(25.0);
        assert_eq!(a.raw, 930);
        assert!((a.celsius - 24.9268).abs() < 1e-3, "{}", a.celsius);
        assert!((TempSensor::AnalogAdc.step_celsius() - 0.0806).abs() < 1e-3);
    }

    #[test]
    fn log_line_format_and_cost() {
        let call = |at_ms, module, level, text| LogCall { at_ms, module, level, text };
        let c = call(1234.567, "sensor", LogLevel::Inf, "temperature 25.06 C");
        assert_eq!(c.line(), "[00:00:01.234,567] <inf> sensor: temperature 25.06 C");
        // 52 characters at 115200 baud: 52 x 10 / 115200 s = 4.51 ms.
        let print_ms = c.line().len() as f64 * 10.0 / 115_200.0 * 1000.0;
        assert_eq!(c.line().len(), 52);
        let calls = [call(10.0, "sensor", LogLevel::Dbg, "raw 0x0191"), call(11.0, "sensor", LogLevel::Inf, "temperature 25.06 C"), call(12.0, "app", LogLevel::Err, "sensor read failed (-5)")];
        // Module "sensor" compiled at INF: the debug call does not exist.
        let levels = [("sensor", 3), ("app", 3)];
        let run = run_logging(&calls, &levels, true, 115_200.0, 4);
        assert_eq!(run.fates[0], LogFate::CompiledOut);
        assert_eq!(run.text_bytes, "temperature 25.06 C".len() + 1 + "sensor read failed (-5)".len() + 1);
        // Immediate: the caller is held for the whole line, and the next call is pushed back.
        assert!((run.held_ms[1] - print_ms).abs() < 1e-9);
        match (run.fates[1], run.fates[2]) {
            (LogFate::Printed(a0, a1), LogFate::Printed(b0, _)) => {
                assert_eq!(a0, 11.0);
                assert!((a1 - (11.0 + print_ms)).abs() < 1e-9);
                assert!((b0 - (12.0 + print_ms)).abs() < 1e-9);
            }
            other => panic!("{other:?}"),
        }
        // Deferred: nobody is held; the lines queue up behind each other on the wire.
        let run = run_logging(&calls, &levels, false, 115_200.0, 4);
        assert_eq!(run.held_ms, [0.0, 0.0, 0.0]);
        match run.fates[2] {
            LogFate::Printed(b0, _) => assert!((b0 - (11.0 + print_ms)).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
        // A buffer for one message: the second call within the print time is dropped.
        assert_eq!(run_logging(&calls, &levels, false, 115_200.0, 1).fates[2], LogFate::Dropped);
    }

    #[test]
    fn bluetooth_builds() {
        assert!((0..4).all(|l| BtBuild::Combined.contains(l)));
        assert_eq!((0..4).map(|l| BtBuild::HostOnly.contains(l)).collect::<Vec<_>>(), [true, true, false, false]);
        assert_eq!((0..4).map(|l| BtBuild::ControllerOnly.contains(l)).collect::<Vec<_>>(), [false, false, true, true]);
        assert!(BtBuild::ControllerOnly.prj_conf().contains("CONFIG_BT_HCI_RAW=y"));
    }
}
