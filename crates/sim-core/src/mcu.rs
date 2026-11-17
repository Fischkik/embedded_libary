//! Concept models of what happens inside a microcontroller.
//!
//! These follow ARM Cortex-M behaviour (the core inside most 32-bit microcontrollers)
//! but use no vendor's register names: the ideas carry over to any chip.

use crate::rng::Rng;

// ---------------------------------------------------------------------------------
// GPIO: a bouncing button and ways to debounce it
// ---------------------------------------------------------------------------------

/// A change of a digital signal: at `t_ms` the level becomes `level`.
pub type Edge = (f64, bool);

/// Level of a signal described by edges at time `t_ms` (`initial` before the first edge).
pub fn level_at(edges: &[Edge], initial: bool, t_ms: f64) -> bool {
    edges.iter().take_while(|e| e.0 <= t_ms).last().map_or(initial, |e| e.1)
}

/// The electrical signal of a push button that is pressed at `press_ms` and released
/// at `release_ms`. `true` means "contact closed". Real contacts chatter for a few
/// milliseconds each time they move: `bounces` extra flips spread over `bounce_ms`.
pub fn bouncy_button(press_ms: f64, release_ms: f64, bounce_ms: f64, bounces: u32, rng: &mut Rng) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (start, target) in [(press_ms, true), (release_ms, false)] {
        let mut times: Vec<f64> = (0..bounces * 2).map(|_| start + rng.range(0.0, bounce_ms.max(1e-6))).collect();
        times.sort_by(f64::total_cmp);
        edges.push((start, target));
        let mut level = target;
        for t in times {
            level = !level;
            edges.push((t, level));
        }
        // An even number of flips ends on the target level again.
    }
    edges
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Debounce {
    /// Trust every sample.
    None,
    /// Accept a new level only after it has been read `samples` times in a row.
    Stable { samples: u32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebounceTrace {
    /// The moments the firmware read the pin, and what it saw.
    pub samples: Vec<Edge>,
    /// The level the firmware believes after debouncing.
    pub accepted: Vec<Edge>,
    /// How many button presses the firmware counted.
    pub presses: u32,
}

/// Firmware reading a pin every `period_ms` and debouncing it.
pub fn sample_and_debounce(edges: &[Edge], period_ms: f64, total_ms: f64, strategy: Debounce) -> DebounceTrace {
    let mut trace = DebounceTrace::default();
    let need = match strategy {
        Debounce::None => 1,
        Debounce::Stable { samples } => samples.max(1),
    };
    let mut accepted = false;
    let mut run = 0u32;
    let mut t = 0.0;
    while t <= total_ms {
        let raw = level_at(edges, false, t);
        trace.samples.push((t, raw));
        if raw != accepted {
            run += 1;
            if run >= need {
                accepted = raw;
                run = 0;
                trace.accepted.push((t, accepted));
                if accepted {
                    trace.presses += 1;
                }
            }
        } else {
            run = 0;
        }
        t += period_ms.max(0.01);
    }
    trace
}

// ---------------------------------------------------------------------------------
// Timer and PWM
// ---------------------------------------------------------------------------------

/// A timer in PWM mode: a counter that counts clock ticks and an output that is
/// high while the counter is below the compare value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PwmTimer {
    /// Clock feeding the timer, Hz.
    pub clk_hz: f64,
    /// Prescaler: the counter advances once every `prescaler + 1` clock cycles.
    pub prescaler: u32,
    /// Auto-reload: the counter runs 0..=reload, then starts again.
    pub reload: u32,
    /// Compare value: output is high while counter < compare.
    pub compare: u32,
    /// Count up and down instead of up only.
    pub center_aligned: bool,
}

impl PwmTimer {
    /// How often the counter advances, Hz.
    pub fn tick_hz(&self) -> f64 {
        self.clk_hz / (self.prescaler as f64 + 1.0)
    }

    /// Counter ticks in one PWM period.
    pub fn period_ticks(&self) -> u64 {
        if self.center_aligned { 2 * self.reload.max(1) as u64 } else { self.reload as u64 + 1 }
    }

    pub fn pwm_hz(&self) -> f64 {
        self.tick_hz() / self.period_ticks() as f64
    }

    /// Fraction of the period the output is high, 0..=1.
    pub fn duty(&self) -> f64 {
        if self.center_aligned {
            (self.compare as f64 / self.reload.max(1) as f64).min(1.0)
        } else {
            (self.compare as f64 / (self.reload as f64 + 1.0)).min(1.0)
        }
    }

    /// Smallest duty-cycle step, as a fraction.
    pub fn duty_step(&self) -> f64 {
        if self.center_aligned { 1.0 / self.reload.max(1) as f64 } else { 1.0 / (self.reload as f64 + 1.0) }
    }

    /// Counter value at tick number `tick`.
    pub fn counter_at(&self, tick: u64) -> u32 {
        let phase = tick % self.period_ticks();
        if self.center_aligned {
            let r = self.reload.max(1) as u64;
            (if phase <= r { phase } else { 2 * r - phase }) as u32
        } else {
            phase as u32
        }
    }

    pub fn output_at(&self, tick: u64) -> bool {
        self.counter_at(tick) < self.compare
    }

    /// Average output voltage if the pin swings between 0 V and `vdd`.
    pub fn average_volts(&self, vdd: f64) -> f64 {
        vdd * self.duty()
    }
}

// ---------------------------------------------------------------------------------
// Interrupts: a model of the Cortex-M NVIC
// ---------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct IrqSource {
    pub name: String,
    /// Lower number = more urgent, as on Cortex-M.
    pub priority: u8,
    pub period_us: f64,
    pub offset_us: f64,
    /// How long the handler runs once it has started.
    pub handler_us: f64,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Normal code running.
    Run,
    /// The CPU saving registers on the stack before a handler starts.
    Entry,
    /// The CPU restoring registers after a handler.
    Exit,
}

/// A stretch of time the CPU spent on one thing. `who` is 0 for the main loop,
/// `i + 1` for interrupt source `i`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub who: usize,
    pub start_us: f64,
    pub end_us: f64,
    pub activity: Activity,
}

/// One interrupt request and what became of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub irq: usize,
    pub raised_us: f64,
    /// When the first instruction of the handler ran. `None` if it never started.
    pub started_us: Option<f64>,
    /// True if the request arrived while the same interrupt was still waiting: it is lost.
    pub lost: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NvicTrace {
    pub segments: Vec<Segment>,
    pub requests: Vec<Request>,
    /// Share of the time left for the main loop, 0..=1.
    pub main_share: f64,
}

impl NvicTrace {
    /// Longest wait from request to first handler instruction for one interrupt.
    pub fn worst_latency_us(&self, irq: usize) -> Option<f64> {
        self.requests
            .iter()
            .filter(|r| r.irq == irq)
            .filter_map(|r| r.started_us.map(|s| s - r.raised_us))
            .max_by(f64::total_cmp)
    }

    pub fn lost(&self, irq: usize) -> usize {
        self.requests.iter().filter(|r| r.irq == irq && r.lost).count()
    }
}

struct Active {
    irq: usize,
    entry_left: f64,
    run_left: f64,
    exit_left: f64,
    request: usize,
}

/// Simulate the main loop being interrupted for `duration_us`.
///
/// Rules modelled (ARMv7-M): a pending interrupt starts only if its priority number is
/// lower than that of the handler currently running (equal does not preempt); entering
/// and leaving a handler costs `entry_us` / `exit_us` for saving and restoring
/// registers; when a handler ends and another interrupt is waiting, the CPU goes
/// straight to it without restoring and saving again ("tail-chaining").
pub fn simulate_nvic(sources: &[IrqSource], duration_us: f64, entry_us: f64, exit_us: f64) -> NvicTrace {
    let mut trace = NvicTrace::default();
    let mut next_raise: Vec<f64> = sources.iter().map(|s| if s.enabled { s.offset_us.max(0.0) } else { f64::INFINITY }).collect();
    // Index into trace.requests for each source's waiting request.
    let mut pending: Vec<Option<usize>> = vec![None; sources.len()];
    let mut stack: Vec<Active> = Vec::new();
    let mut t = 0.0f64;
    let mut main_time = 0.0;
    let mut guard = 0;

    while t < duration_us && guard < 200_000 {
        guard += 1;
        // 1. Raise every request that is due.
        for i in 0..sources.len() {
            while next_raise[i] <= t {
                let lost = pending[i].is_some();
                trace.requests.push(Request { irq: i, raised_us: next_raise[i], started_us: None, lost });
                if !lost {
                    pending[i] = Some(trace.requests.len() - 1);
                }
                next_raise[i] += sources[i].period_us.max(1.0);
            }
        }
        // 2. Start the most urgent waiting interrupt if it beats what is running.
        let current_priority = stack.last().map_or(u16::MAX, |a| sources[a.irq].priority as u16);
        let best = (0..sources.len()).filter(|&i| pending[i].is_some()).min_by_key(|&i| (sources[i].priority, i));
        if let Some(i) = best {
            // A handler that is in its exit sequence is finished: anything may follow it.
            let top_finishing = stack.last().is_some_and(|a| a.run_left <= 0.0);
            if (sources[i].priority as u16) < current_priority || top_finishing {
                let tail_chain = top_finishing;
                if tail_chain {
                    stack.pop();
                }
                let request = pending[i].take().unwrap_or(0);
                stack.push(Active {
                    irq: i,
                    entry_left: if tail_chain { entry_us * 0.5 } else { entry_us },
                    run_left: sources[i].handler_us.max(0.1),
                    exit_left: exit_us,
                    request,
                });
            }
        }
        // 3. Run until the next thing happens.
        let next_event = next_raise.iter().copied().fold(duration_us, f64::min);
        let (who, activity, left) = match stack.last() {
            None => (0, Activity::Run, f64::INFINITY),
            Some(a) if a.entry_left > 0.0 => (a.irq + 1, Activity::Entry, a.entry_left),
            Some(a) if a.run_left > 0.0 => (a.irq + 1, Activity::Run, a.run_left),
            Some(a) => (a.irq + 1, Activity::Exit, a.exit_left),
        };
        let dt = left.min(next_event - t).max(0.0);
        let end = t + dt;
        if dt > 0.0 {
            match trace.segments.last_mut() {
                Some(s) if s.who == who && s.activity == activity && (s.end_us - t).abs() < 1e-9 => s.end_us = end,
                _ => trace.segments.push(Segment { who, start_us: t, end_us: end, activity }),
            }
            if who == 0 {
                main_time += dt;
            }
        }
        if let Some(a) = stack.last_mut() {
            if a.entry_left > 0.0 {
                a.entry_left -= dt;
                if a.entry_left <= 1e-9 {
                    a.entry_left = 0.0;
                    trace.requests[a.request].started_us = Some(end);
                }
            } else if a.run_left > 0.0 {
                a.run_left -= dt;
                if a.run_left <= 1e-9 {
                    a.run_left = 0.0;
                }
            } else {
                a.exit_left -= dt;
                if a.exit_left <= 1e-9 {
                    stack.pop();
                }
            }
        }
        t = end;
        if dt == 0.0 && left.is_infinite() && next_event <= t {
            // Nothing running and an event exactly now: loop again to raise it.
            continue;
        }
    }
    trace.main_share = if duration_us > 0.0 { main_time / duration_us } else { 1.0 };
    trace
}

// ---------------------------------------------------------------------------------
// ADC: turning a voltage into a number by successive approximation
// ---------------------------------------------------------------------------------

/// An ideal analog-to-digital converter: it sorts a voltage between 0 V and `vref`
/// into one of `2^bits` equal steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adc {
    /// Resolution: the result is a number with this many bits.
    pub bits: u32,
    /// Reference voltage: the top of the measuring range, volts.
    pub vref: f64,
}

/// One question of the successive-approximation search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SarStep {
    /// Which bit is being decided (bits - 1 = most significant, asked first).
    pub bit: u32,
    /// The voltage the converter compares the input with.
    pub trial_volts: f64,
    /// True if the input was at or above the trial voltage: the bit stays 1.
    pub keep: bool,
    /// The range the input is known to lie in after this question, volts.
    pub known_lo: f64,
    pub known_hi: f64,
}

impl Adc {
    /// Number of different results.
    pub fn levels(&self) -> u32 {
        1u32 << self.bits.clamp(1, 24)
    }

    /// Width of one step in volts ("one LSB").
    pub fn lsb(&self) -> f64 {
        self.vref / self.levels() as f64
    }

    /// The number the converter reports for `volts`. Inputs outside 0..vref are clipped.
    pub fn convert(&self, volts: f64) -> u32 {
        let code = (volts / self.lsb()).floor();
        code.clamp(0.0, (self.levels() - 1) as f64) as u32
    }

    /// The voltage firmware computes back from a result: the bottom of that step.
    pub fn volts(&self, code: u32) -> f64 {
        code as f64 * self.lsb()
    }

    /// The search a successive-approximation converter performs: one comparison per
    /// bit, most significant bit first, each halving the range the input can be in.
    pub fn sar_steps(&self, volts: f64) -> Vec<SarStep> {
        let bits = self.bits.clamp(1, 24);
        let mut code = 0u32;
        let (mut lo, mut hi) = (0.0, self.vref);
        (0..bits)
            .rev()
            .map(|bit| {
                let trial = code | (1 << bit);
                let trial_volts = self.volts(trial);
                let keep = volts >= trial_volts;
                if keep {
                    code = trial;
                    lo = trial_volts;
                } else {
                    hi = trial_volts;
                }
                SarStep { bit, trial_volts, keep, known_lo: lo, known_hi: hi }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------------
// RTOS: fixed-priority preemptive scheduling of periodic tasks
// ---------------------------------------------------------------------------------

/// A task that wakes up every `period_ms`, needs the processor for `work_ms`, and
/// then sleeps until its next period.
#[derive(Clone, Debug, PartialEq)]
pub struct RtosTask {
    pub name: String,
    /// Lower number = more urgent (the same way round as the interrupt model above).
    pub priority: u8,
    pub period_ms: f64,
    /// Processor time one activation needs.
    pub work_ms: f64,
    pub enabled: bool,
}

/// One activation of a task. Its deadline is the task's next wake-up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Job {
    pub task: usize,
    pub released_ms: f64,
    pub deadline_ms: f64,
    /// `None` if it was still unfinished when the simulation ended.
    pub finished_ms: Option<f64>,
}

/// A stretch of time the processor spent on one thing: `who` is 0 for "nothing to do"
/// (the idle task), `i + 1` for task `i`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    pub who: usize,
    pub start_ms: f64,
    pub end_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RtosTrace {
    pub slices: Vec<Slice>,
    pub jobs: Vec<Job>,
    /// Share of the time nothing needed the processor, 0..=1.
    pub idle_share: f64,
    pub duration_ms: f64,
}

impl RtosTrace {
    /// Longest time from a wake-up to that activation being finished.
    pub fn worst_response_ms(&self, task: usize) -> Option<f64> {
        self.jobs.iter().filter(|j| j.task == task).filter_map(|j| j.finished_ms.map(|f| f - j.released_ms)).max_by(f64::total_cmp)
    }

    /// True if this activation was not finished by its deadline.
    pub fn is_missed(&self, job: &Job) -> bool {
        match job.finished_ms {
            Some(f) => f > job.deadline_ms + 1e-6,
            None => job.deadline_ms <= self.duration_ms + 1e-6,
        }
    }

    pub fn missed(&self, task: usize) -> usize {
        self.jobs.iter().filter(|j| j.task == task && self.is_missed(j)).count()
    }
}

/// Share of the processor the enabled tasks need in total: the sum of work / period.
pub fn utilisation(tasks: &[RtosTask]) -> f64 {
    tasks.iter().filter(|t| t.enabled).map(|t| t.work_ms / t.period_ms).sum()
}

/// Liu and Layland's bound: `n` periodic tasks whose priorities follow their periods
/// (shortest period = most urgent) never miss a deadline if their utilisation is at
/// most `n (2^(1/n) - 1)`. Above it they may or may not.
pub fn rate_monotonic_bound(n: usize) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let n = n as f64;
    n * (2f64.powf(1.0 / n) - 1.0)
}

/// Simulate a fixed-priority preemptive scheduler for `duration_ms`.
///
/// Rule modelled: at every instant the most urgent task that has work to do runs.
/// All tasks wake at time 0 (the worst case) and then once per period. Between tasks
/// of equal priority the one that has waited longest runs, and it is not interrupted
/// by its equals. A task that overruns its period starts its next activation late.
pub fn simulate_rtos(tasks: &[RtosTask], duration_ms: f64) -> RtosTrace {
    let n = tasks.len();
    let mut trace = RtosTrace { duration_ms, ..Default::default() };
    let mut released = vec![0u32; n];
    let next_release = |i: usize, released: &[u32]| if tasks[i].enabled { released[i] as f64 * tasks[i].period_ms.max(0.1) } else { f64::INFINITY };
    // Per task: activations not finished yet, as (index into trace.jobs, work left).
    let mut queues: Vec<std::collections::VecDeque<(usize, f64)>> = vec![Default::default(); n];
    let mut t = 0.0f64;
    let mut idle = 0.0;
    let mut guard = 0;

    while t < duration_ms - 1e-9 && guard < 200_000 {
        guard += 1;
        for i in 0..n {
            while next_release(i, &released) <= t + 1e-9 {
                let at = next_release(i, &released);
                released[i] += 1;
                trace.jobs.push(Job { task: i, released_ms: at, deadline_ms: next_release(i, &released), finished_ms: None });
                queues[i].push_back((trace.jobs.len() - 1, tasks[i].work_ms.max(0.01)));
            }
        }
        let key = |i: usize| (tasks[i].priority, trace.jobs[queues[i][0].0].released_ms);
        let running = (0..n).filter(|&i| !queues[i].is_empty()).min_by(|&a, &b| {
            let (ka, kb) = (key(a), key(b));
            ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(a.cmp(&b))
        });
        let next_event = (0..n).map(|i| next_release(i, &released)).fold(duration_ms, f64::min);
        let (who, dt) = match running {
            None => (0, next_event - t),
            Some(i) => (i + 1, queues[i][0].1.min(next_event - t)),
        };
        let end = t + dt;
        match running {
            None => idle += dt,
            Some(i) => {
                queues[i][0].1 -= dt;
                if queues[i][0].1 <= 1e-9 {
                    let (job, _) = queues[i].pop_front().unwrap_or_default();
                    trace.jobs[job].finished_ms = Some(end);
                }
            }
        }
        match trace.slices.last_mut() {
            Some(s) if s.who == who && (s.end_ms - t).abs() < 1e-9 => s.end_ms = end,
            _ => trace.slices.push(Slice { who, start_ms: t, end_ms: end }),
        }
        t = end;
    }
    trace.idle_share = if duration_ms > 0.0 { idle / duration_ms } else { 1.0 };
    trace
}

// ---------------------------------------------------------------------------------
// Low power: sleeping between short bursts of work
// ---------------------------------------------------------------------------------

/// A device that wakes up once per `period_s`, works for `work_s`, and sleeps for the
/// rest. Waking up takes `wake_s`, during which it already draws the run current.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SleepCycle {
    pub period_s: f64,
    pub work_s: f64,
    pub wake_s: f64,
    /// Current while awake, amperes.
    pub run_a: f64,
    /// Current while asleep, amperes.
    pub sleep_a: f64,
}

impl SleepCycle {
    /// Seconds per period spent `[working, waking up, asleep]`. If waking and working
    /// do not fit into the period, the device never sleeps.
    pub fn times_s(&self) -> [f64; 3] {
        let wake = self.wake_s.clamp(0.0, self.period_s);
        let work = self.work_s.clamp(0.0, self.period_s - wake);
        [work, wake, self.period_s - wake - work]
    }

    /// Charge per period used for `[working, waking up, sleeping]`, in coulombs
    /// (ampere-seconds).
    pub fn charge_c(&self) -> [f64; 3] {
        let [work, wake, asleep] = self.times_s();
        [work * self.run_a, wake * self.run_a, asleep * self.sleep_a]
    }

    /// Average current over a whole period, amperes.
    pub fn average_a(&self) -> f64 {
        self.charge_c().iter().sum::<f64>() / self.period_s
    }

    /// How long a battery of `capacity_mah` milliampere-hours lasts, in seconds.
    pub fn battery_life_s(&self, capacity_mah: f64) -> f64 {
        // 1 mAh = 1 mA for 3600 s = 3.6 ampere-seconds.
        capacity_mah * 3.6 / self.average_a()
    }
}

// ---------------------------------------------------------------------------------
// Watchdog: a timer that resets the chip unless the firmware keeps restarting it
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Watchdog {
    /// The watchdog resets the chip if it is not restarted ("kicked") for this long.
    pub timeout_ms: f64,
    /// One pass of the main loop. The loop kicks the watchdog at the end of each pass.
    pub loop_ms: f64,
    /// Time from a reset until the main loop runs again.
    pub boot_ms: f64,
    /// The moment the firmware gets stuck (it happens once).
    pub hang_at_ms: Option<f64>,
    /// The mistake: kick from a timer interrupt, which keeps firing while the loop is stuck.
    pub kick_from_timer: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwState {
    Booting,
    Running,
    Hung,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WatchdogTrace {
    /// `[time, time left before the watchdog bites]`.
    pub counter: Vec<[f64; 2]>,
    pub kicks: Vec<f64>,
    pub resets: Vec<f64>,
    pub states: Vec<(FwState, f64, f64)>,
    /// Share of the time the firmware did its job, 0..=1.
    pub working_share: f64,
}

pub fn simulate_watchdog(w: &Watchdog, duration_ms: f64) -> WatchdogTrace {
    let mut tr = WatchdogTrace::default();
    let timeout = w.timeout_ms.max(1.0);
    let mut hang = w.hang_at_ms;
    let mut t = 0.0;
    let mut working = 0.0;
    let mut guard = 0;
    'boot: while t < duration_ms && guard < 100_000 {
        // Booting: the watchdog starts counting when the main loop starts.
        let start = (t + w.boot_ms.max(0.1)).min(duration_ms);
        tr.states.push((FwState::Booting, t, start));
        tr.counter.extend([[t, timeout], [start, timeout]]);
        t = start;
        let mut last_kick = t;
        let mut next_loop_kick = t + w.loop_ms.max(0.1);
        let mut state_from = t;
        let mut hung = false;
        while t < duration_ms && guard < 100_000 {
            guard += 1;
            let expiry = last_kick + timeout;
            let next_kick = if w.kick_from_timer { last_kick + timeout / 2.0 } else if hung { f64::INFINITY } else { next_loop_kick };
            let hang_at = if hung { f64::INFINITY } else { hang.map_or(f64::INFINITY, |h| h.max(t)) };
            if hang_at <= next_kick.min(expiry).min(duration_ms) {
                tr.states.push((FwState::Running, state_from, hang_at));
                working += hang_at - state_from;
                (t, state_from, hung, hang) = (hang_at, hang_at, true, None);
            } else if next_kick < expiry && next_kick < duration_ms {
                tr.counter.extend([[next_kick, expiry - next_kick], [next_kick, timeout]]);
                tr.kicks.push(next_kick);
                (t, last_kick) = (next_kick, next_kick);
                next_loop_kick = next_kick + w.loop_ms.max(0.1);
            } else if expiry <= duration_ms {
                tr.counter.push([expiry, 0.0]);
                tr.resets.push(expiry);
                tr.states.push((if hung { FwState::Hung } else { FwState::Running }, state_from, expiry));
                if !hung {
                    working += expiry - state_from;
                }
                t = expiry;
                continue 'boot;
            } else {
                tr.counter.push([duration_ms, expiry - duration_ms]);
                tr.states.push((if hung { FwState::Hung } else { FwState::Running }, state_from, duration_ms));
                if !hung {
                    working += duration_ms - state_from;
                }
                t = duration_ms;
            }
        }
    }
    tr.working_share = if duration_ms > 0.0 { working / duration_ms } else { 0.0 };
    tr
}

// ---------------------------------------------------------------------------------
// Priority inversion: tasks sharing a lock
// ---------------------------------------------------------------------------------

/// A task that runs once: it becomes ready at `arrive_ms` and then works through
/// `script`, a list of `(milliseconds of work, needs the shared lock?)`.
#[derive(Clone, Debug, PartialEq)]
pub struct LockTask {
    pub name: String,
    /// Lower number = more urgent.
    pub priority: u8,
    pub arrive_ms: f64,
    pub script: Vec<(f64, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    Running,
    /// Could run, but a more urgent task has the processor.
    Ready,
    /// Waiting for the lock, which another task holds.
    Blocked,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LockTrace {
    /// `(task, state, from, to)`.
    pub spans: Vec<(usize, LockState, f64, f64)>,
    /// `(task, from, to)`: who held the lock.
    pub held: Vec<(usize, f64, f64)>,
    pub finished_ms: Vec<Option<f64>>,
}

/// Fixed-priority preemptive scheduling of tasks that share one lock (a mutex), in
/// steps of 0.1 ms. With `inheritance`, a task that holds the lock runs at the priority
/// of the most urgent task waiting for it (the priority inheritance protocol).
pub fn simulate_lock(tasks: &[LockTask], inheritance: bool, duration_ms: f64) -> LockTrace {
    const TICK: f64 = 0.1;
    let ticks = |ms: f64| (ms / TICK).round() as i64;
    let n = tasks.len();
    let mut trace = LockTrace { finished_ms: vec![None; n], ..Default::default() };
    // Per task: (current script entry, ticks left in it).
    let mut at: Vec<(usize, i64)> = tasks.iter().map(|t| (0, t.script.first().map_or(0, |s| ticks(s.0)))).collect();
    let mut last_span: Vec<Option<usize>> = vec![None; n];
    let mut owner: Option<usize> = None;
    for k in 0..ticks(duration_ms) {
        let (t0, t1) = (k as f64 * TICK, (k + 1) as f64 * TICK);
        for (i, task) in tasks.iter().enumerate() {
            while at[i].0 < task.script.len() && at[i].1 <= 0 {
                at[i].0 += 1;
                at[i].1 = task.script.get(at[i].0).map_or(0, |s| ticks(s.0));
            }
        }
        let active: Vec<usize> = (0..n).filter(|&i| ticks(tasks[i].arrive_ms) <= k && at[i].0 < tasks[i].script.len()).collect();
        let needs = |i: usize| tasks[i].script[at[i].0].1;
        let blocked = |i: usize, owner: Option<usize>| needs(i) && owner.is_some_and(|o| o != i);
        let effective = |i: usize| {
            let waiting = active.iter().filter(|&&j| blocked(j, owner)).map(|&j| tasks[j].priority).min();
            match waiting {
                Some(p) if inheritance && owner == Some(i) => p.min(tasks[i].priority),
                _ => tasks[i].priority,
            }
        };
        let running = active.iter().copied().filter(|&i| !blocked(i, owner)).min_by_key(|&i| (effective(i), i));
        if let Some(r) = running {
            if needs(r) {
                owner = Some(r);
            }
        }
        for &i in &active {
            let state = if running == Some(i) { LockState::Running } else if blocked(i, owner) { LockState::Blocked } else { LockState::Ready };
            match last_span[i].map(|s| &mut trace.spans[s]) {
                Some(s) if s.1 == state && (s.3 - t0).abs() < 1e-6 => s.3 = t1,
                _ => {
                    trace.spans.push((i, state, t0, t1));
                    last_span[i] = Some(trace.spans.len() - 1);
                }
            }
        }
        if let Some(o) = owner {
            match trace.held.last_mut() {
                Some(h) if h.0 == o && (h.2 - t0).abs() < 1e-6 => h.2 = t1,
                _ => trace.held.push((o, t0, t1)),
            }
        }
        if let Some(r) = running {
            let in_critical_section = needs(r);
            at[r].1 -= 1;
            if at[r].1 <= 0 {
                if in_critical_section {
                    owner = None;
                }
                if at[r].0 + 1 >= tasks[r].script.len() {
                    at[r].0 += 1;
                    trace.finished_ms[r] = Some(t1);
                }
            }
        }
    }
    trace
}

// ---------------------------------------------------------------------------------
// RAM layout: global variables, heap and stack
// ---------------------------------------------------------------------------------

/// Where RAM starts on an ARM Cortex-M (ARMv7-M address map: SRAM region).
pub const RAM_BASE: u32 = 0x2000_0000;

/// The classic layout of a microcontroller's RAM: global variables at the bottom, the
/// heap growing upward above them, the stack growing downward from the top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RamLayout {
    pub ram_bytes: u32,
    pub globals_bytes: u32,
    pub heap_bytes: u32,
    /// How many function calls are nested inside each other right now.
    pub call_depth: u32,
    /// Stack bytes one call needs (saved registers, return address, local variables).
    pub frame_bytes: u32,
}

impl RamLayout {
    pub fn stack_bytes(&self) -> u32 {
        self.call_depth.saturating_mul(self.frame_bytes)
    }

    /// Bytes between the top of the heap and the stack pointer. Negative: they overlap.
    pub fn free_bytes(&self) -> i64 {
        self.ram_bytes as i64 - self.globals_bytes as i64 - self.heap_bytes as i64 - self.stack_bytes() as i64
    }

    pub fn collided(&self) -> bool {
        self.free_bytes() < 0
    }

    pub fn heap_start(&self) -> u32 {
        RAM_BASE + self.globals_bytes
    }

    pub fn heap_end(&self) -> u32 {
        self.heap_start() + self.heap_bytes
    }

    pub fn stack_top(&self) -> u32 {
        RAM_BASE + self.ram_bytes
    }

    pub fn stack_pointer(&self) -> u32 {
        self.stack_top().saturating_sub(self.stack_bytes())
    }
}

// ---------------------------------------------------------------------------------
// Clock tree: from an oscillator to the clocks the chip runs on
// ---------------------------------------------------------------------------------

/// A simple clock tree: source -> optional PLL (divide, then multiply) -> system
/// divider -> processor clock -> bus divider -> peripheral clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockTree {
    pub source_hz: f64,
    /// How far the source may be off, in parts per million (10 000 ppm = 1 %).
    pub tolerance_ppm: f64,
    pub pll_on: bool,
    pub pll_div: u32,
    pub pll_mul: u32,
    pub sys_div: u32,
    pub bus_div: u32,
}

impl ClockTree {
    /// Frequency after the PLL (or the source itself if the PLL is bypassed).
    pub fn pll_hz(&self) -> f64 {
        if self.pll_on { self.source_hz / self.pll_div.max(1) as f64 * self.pll_mul.max(1) as f64 } else { self.source_hz }
    }

    pub fn cpu_hz(&self) -> f64 {
        self.pll_hz() / self.sys_div.max(1) as f64
    }

    pub fn bus_hz(&self) -> f64 {
        self.cpu_hz() / self.bus_div.max(1) as f64
    }

    /// Seconds a clock built on this source can be off after one day.
    pub fn drift_s_per_day(&self) -> f64 {
        self.tolerance_ppm * 1e-6 * 86_400.0
    }
}

/// The largest clock mismatch between two ends of a serial (UART) link with
/// `bits_per_frame` bits per byte before the last bit is sampled outside its slot: the
/// receiver samples each bit in its middle, so after `bits - 0.5` bit times the error
/// must still be under half a bit.
pub fn uart_mismatch_limit(bits_per_frame: u32) -> f64 {
    0.5 / (bits_per_frame as f64 - 0.5)
}

// ---------------------------------------------------------------------------------
// DMA: receiving a stream of bytes with and without the processor's help
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ByteStream {
    /// Bits per second on the line; one byte takes 10 bits (start, 8 data, stop).
    pub baud: f64,
    pub cpu_hz: f64,
    /// Clock cycles to enter an interrupt, store one byte and return.
    pub cycles_per_byte_irq: f64,
    /// Bytes the DMA hardware collects before it interrupts the processor.
    pub block_bytes: u32,
    /// Clock cycles of the handler that runs once per finished block.
    pub cycles_per_block_irq: f64,
    /// Longest time something else keeps the processor from answering an interrupt.
    pub blocked_us: f64,
}

impl ByteStream {
    pub fn bytes_per_s(&self) -> f64 {
        self.baud / 10.0
    }

    pub fn byte_time_us(&self) -> f64 {
        1e6 / self.bytes_per_s()
    }

    /// Share of the processor used when every byte causes an interrupt (can exceed 1).
    pub fn irq_load(&self) -> f64 {
        self.bytes_per_s() * self.cycles_per_byte_irq / self.cpu_hz
    }

    /// Share of the processor used with DMA: one interrupt per block.
    pub fn dma_load(&self) -> f64 {
        self.bytes_per_s() / self.block_bytes.max(1) as f64 * self.cycles_per_block_irq / self.cpu_hz
    }

    /// Time one per-byte interrupt keeps the processor busy, microseconds.
    pub fn irq_time_us(&self) -> f64 {
        self.cycles_per_byte_irq / self.cpu_hz * 1e6
    }

    /// Without DMA the receiver can hold one finished byte. If the processor does not
    /// fetch it before the next one is complete, a byte is lost.
    pub fn irq_loses_bytes(&self) -> bool {
        self.blocked_us + self.irq_time_us() > self.byte_time_us() || self.irq_load() >= 1.0
    }
}

// ---------------------------------------------------------------------------------
// Firmware update: surviving a power cut and a bad image
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Empty,
    /// A complete, valid firmware of this version.
    Valid(u8),
    /// A half-written or damaged firmware.
    Broken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateScenario {
    /// True: bootloader with two slots. False: the new firmware overwrites the old one.
    pub two_slots: bool,
    pub power_cut_while_writing: bool,
    pub download_corrupted: bool,
    /// The new version passes the checksum but does not work.
    pub new_version_faulty: bool,
}

/// One moment of the update: what is in flash and what is running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateStep {
    pub title: &'static str,
    pub note: &'static str,
    /// The slot the firmware runs from.
    pub slot_a: Slot,
    /// The spare slot (unused without a bootloader).
    pub slot_b: Slot,
    /// Version that is running, `None` if the device is dead.
    pub running: Option<u8>,
}

/// Play an update from version 1 to version 2.
pub fn firmware_update(s: UpdateScenario) -> Vec<UpdateStep> {
    use Slot::*;
    let step = |title, note, slot_a, slot_b, running| UpdateStep { title, note, slot_a, slot_b, running };
    let mut out = vec![step("Before", "Version 1 is running.", Valid(1), Empty, Some(1))];
    if !s.two_slots {
        if s.power_cut_while_writing {
            out.push(step("Writing version 2 over version 1", "The power fails halfway.", Broken, Empty, None));
            out.push(step("Power returns", "Flash holds half of each version. Nothing can run, and nothing is left to repair it: the device is dead.", Broken, Empty, None));
        } else if s.download_corrupted {
            out.push(step("Writing version 2 over version 1", "Some bytes arrived damaged. The old version is already gone.", Broken, Empty, None));
            out.push(step("Restart", "The damaged firmware crashes. The device is dead.", Broken, Empty, None));
        } else if s.new_version_faulty {
            out.push(step("Writing version 2 over version 1", "Complete.", Valid(2), Empty, None));
            out.push(step("Restart", "Version 2 starts but does not work, and there is no way back to version 1.", Valid(2), Empty, None));
        } else {
            out.push(step("Writing version 2 over version 1", "Complete.", Valid(2), Empty, None));
            out.push(step("Restart", "Version 2 is running. It went well - this time.", Valid(2), Empty, Some(2)));
        }
        return out;
    }
    if s.power_cut_while_writing {
        out.push(step("Downloading version 2 into the spare slot", "Version 1 keeps running meanwhile. The power fails halfway.", Valid(1), Broken, Some(1)));
        out.push(step("Power returns", "The bootloader finds a valid version 1 and starts it. The download simply starts again.", Valid(1), Broken, Some(1)));
        return out;
    }
    if s.download_corrupted {
        out.push(step("Downloading version 2 into the spare slot", "Version 1 keeps running meanwhile. Some bytes arrived damaged.", Valid(1), Broken, Some(1)));
        out.push(step("Checking the download", "The checksum does not match: the image is rejected and version 1 carries on.", Valid(1), Broken, Some(1)));
        return out;
    }
    out.push(step("Downloading version 2 into the spare slot", "Version 1 keeps running meanwhile.", Valid(1), Valid(2), Some(1)));
    out.push(step("Checking the download", "Checksum and signature match. Version 2 is marked \"try this once\".", Valid(1), Valid(2), Some(1)));
    out.push(step("Restart: the bootloader swaps the slots", "Version 2 runs on trial. Version 1 is kept in the spare slot.", Valid(2), Valid(1), Some(2)));
    if s.new_version_faulty {
        out.push(step("Version 2 never confirms", "It hangs; the watchdog resets the chip.", Valid(2), Valid(1), None));
        out.push(step("Restart: the bootloader swaps back", "No confirmation was stored, so version 1 is restored and runs again.", Valid(1), Valid(2), Some(1)));
    } else {
        out.push(step("Version 2 confirms itself", "It has started and reached its server: it marks itself as good. The update is permanent.", Valid(2), Valid(1), Some(2)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adc_known_answers() {
        let adc = Adc { bits: 12, vref: 3.3 };
        assert_eq!(adc.levels(), 4096);
        assert!((adc.lsb() - 805.664e-6).abs() < 1e-9, "{}", adc.lsb());
        assert_eq!(adc.convert(0.0), 0);
        assert_eq!(adc.convert(1.65), 2048);
        assert_eq!(adc.convert(3.3), 4095, "full scale clips to the highest code");
        assert_eq!(adc.convert(5.0), 4095);
        assert_eq!(adc.convert(-1.0), 0);
        assert_eq!(adc.volts(2048), 1.65);
    }

    #[test]
    fn sar_search_matches_direct_conversion() {
        // 8 bits, 1 V reference, 0.425 V in: 0.5 V is too high, 0.25 V is kept, ...
        // floor(0.425 x 256) = 108 = 0b0110_1100.
        let adc = Adc { bits: 8, vref: 1.0 };
        let steps = adc.sar_steps(0.425);
        assert_eq!(steps[0].trial_volts, 0.5);
        assert_eq!(steps[1].trial_volts, 0.25);
        let kept: Vec<bool> = steps.iter().map(|s| s.keep).collect();
        assert_eq!(kept, [false, true, true, false, true, true, false, false]);
        assert_eq!(adc.convert(0.425), 0b0110_1100);
        let last = steps[7];
        assert!((last.known_hi - last.known_lo - adc.lsb()).abs() < 1e-12, "the search ends one step wide");
        assert!(last.known_lo <= 0.425 && 0.425 < last.known_hi);

        // The bit-by-bit search and the formula agree for every input.
        let mut rng = Rng::new(3);
        for bits in [1, 3, 6, 10, 12] {
            let adc = Adc { bits, vref: 3.3 };
            for _ in 0..500 {
                let v = rng.range(-0.2, 3.6);
                let code = adc.sar_steps(v).iter().fold(0u32, |c, s| c | ((s.keep as u32) << s.bit));
                assert_eq!(code, adc.convert(v), "{bits} bits, {v} V");
                if (0.0..3.3).contains(&v) {
                    let err = v - adc.volts(code);
                    assert!((0.0..adc.lsb()).contains(&err), "error below one step: {err}");
                }
            }
        }
    }

    fn task(name: &str, priority: u8, period: f64, work: f64) -> RtosTask {
        RtosTask { name: name.into(), priority, period_ms: period, work_ms: work, enabled: true }
    }

    #[test]
    fn a_lone_task_runs_at_once_and_the_rest_is_idle() {
        let trace = simulate_rtos(&[task("a", 1, 10.0, 2.0)], 100.0);
        assert_eq!(trace.jobs.len(), 10);
        assert!((trace.idle_share - 0.8).abs() < 1e-9);
        assert!((trace.worst_response_ms(0).unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(trace.missed(0), 0);
    }

    #[test]
    fn response_time_matches_the_textbook_recurrence() {
        // R = C + sum over more urgent tasks of ceil(R / T_j) x C_j.
        // fast: C = 1, T = 5. slow: C = 8, T = 20.  R = 8 + ceil(R / 5) x 1  ->  R = 10.
        let tasks = [task("fast", 1, 5.0, 1.0), task("slow", 2, 20.0, 8.0)];
        let trace = simulate_rtos(&tasks, 100.0);
        assert!((trace.worst_response_ms(0).unwrap() - 1.0).abs() < 1e-9);
        assert!((trace.worst_response_ms(1).unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(trace.missed(0) + trace.missed(1), 0);
        assert!((utilisation(&tasks) - 0.6).abs() < 1e-12);
        assert!((trace.idle_share - 0.4).abs() < 1e-9);
        // "slow" is split by "fast": it runs 1..5 and 6..10.
        let slow: Vec<(f64, f64)> = trace.slices.iter().filter(|s| s.who == 2 && s.end_ms <= 10.0).map(|s| (s.start_ms, s.end_ms)).collect();
        assert_eq!(slow, [(1.0, 5.0), (6.0, 10.0)]);
    }

    #[test]
    fn wrong_priorities_miss_deadlines_at_a_load_that_is_otherwise_fine() {
        // 70 % load. Shortest period most urgent: fine.
        let good = [task("fast", 1, 5.0, 2.0), task("slow", 2, 20.0, 6.0)];
        let trace = simulate_rtos(&good, 100.0);
        assert_eq!(trace.missed(0) + trace.missed(1), 0);
        // The other way round, "fast" waits 6 ms for "slow" and misses its 5 ms deadline.
        let bad = [task("fast", 2, 5.0, 2.0), task("slow", 1, 20.0, 6.0)];
        let trace = simulate_rtos(&bad, 100.0);
        assert!(trace.missed(0) > 0);
        assert_eq!(trace.missed(1), 0);
        // Equal priority: no preemption either, so "fast" still waits for "slow".
        let equal = [task("slow", 1, 20.0, 6.0), task("fast", 1, 5.0, 2.0)];
        assert!(simulate_rtos(&equal, 100.0).missed(1) > 0);
    }

    #[test]
    fn overload_starves_the_least_urgent_task() {
        let tasks = [task("a", 1, 5.0, 3.0), task("b", 2, 10.0, 3.0), task("c", 3, 20.0, 5.0)];
        assert!(utilisation(&tasks) > 1.0);
        let trace = simulate_rtos(&tasks, 200.0);
        assert_eq!(trace.missed(0), 0);
        assert!(trace.missed(2) > 0);
        assert!(trace.idle_share < 1e-9);
    }

    #[test]
    fn rate_monotonic_bound_values() {
        assert_eq!(rate_monotonic_bound(1), 1.0);
        assert!((rate_monotonic_bound(2) - 0.8284).abs() < 1e-4);
        assert!((rate_monotonic_bound(3) - 0.7798).abs() < 1e-4);
        assert!((rate_monotonic_bound(10_000) - std::f64::consts::LN_2).abs() < 1e-4);
    }

    #[test]
    fn sleep_cycle_average_current_and_battery_life() {
        // 5 ms at 4 mA every 10 s, 1.3 µA asleep:
        // (4e-3 x 0.005 + 1.3e-6 x 9.995) / 10 = 3.29935 µA.
        let c = SleepCycle { period_s: 10.0, work_s: 5e-3, wake_s: 0.0, run_a: 4e-3, sleep_a: 1.3e-6 };
        assert!((c.average_a() - 3.29935e-6).abs() < 1e-12, "{}", c.average_a());
        // 225 mAh / 3.29935 µA = 68 195 hours.
        assert!((c.battery_life_s(225.0) / 3600.0 - 68_195.0).abs() < 1.0);
        // Never sleeping: the average is the run current, 225 mAh / 4 mA = 56.25 h.
        let awake = SleepCycle { work_s: 20.0, ..c };
        assert_eq!(awake.times_s(), [10.0, 0.0, 0.0]);
        assert!((awake.battery_life_s(225.0) / 3600.0 - 56.25).abs() < 1e-9);
        // A 2 ms wake-up costs 8 µC per cycle. It pays off against a sleep current that
        // is 0.9 µA lower only if the cycle is longer than about 8.9 s.
        let deep = |period_s| SleepCycle { period_s, wake_s: 2e-3, sleep_a: 0.4e-6, ..c };
        assert!(deep(5.0).average_a() > SleepCycle { period_s: 5.0, ..c }.average_a());
        assert!(deep(20.0).average_a() < SleepCycle { period_s: 20.0, ..c }.average_a());
    }

    #[test]
    fn bouncing_is_miscounted_without_debounce_and_counted_once_with_it() {
        let edges = bouncy_button(10.0, 60.0, 4.0, 4, &mut Rng::new(5));
        assert!(edges.len() > 4);
        assert!(level_at(&edges, false, 30.0), "settled pressed");
        assert!(!level_at(&edges, false, 90.0), "settled released");
        let raw = sample_and_debounce(&edges, 0.2, 100.0, Debounce::None);
        assert!(raw.presses > 1, "{}", raw.presses);
        let good = sample_and_debounce(&edges, 1.0, 100.0, Debounce::Stable { samples: 8 });
        assert_eq!(good.presses, 1);
    }

    #[test]
    fn pwm_frequency_and_duty() {
        let t = PwmTimer { clk_hz: 48e6, prescaler: 47, reload: 999, compare: 250, center_aligned: false };
        assert_eq!(t.tick_hz(), 1e6);
        assert_eq!(t.pwm_hz(), 1000.0);
        assert_eq!(t.duty(), 0.25);
        let high = (0..1000).filter(|&k| t.output_at(k)).count();
        assert_eq!(high, 250);
        assert_eq!(PwmTimer { compare: 5000, ..t }.duty(), 1.0);

        let c = PwmTimer { center_aligned: true, reload: 500, ..t };
        assert_eq!(c.pwm_hz(), 1000.0);
        assert_eq!(c.duty(), 0.5);
        let high = (0..1000).filter(|&k| c.output_at(k)).count();
        assert!((high as i32 - 500).abs() <= 1, "{high}");
        assert_eq!(c.counter_at(500), 500);
        assert_eq!(c.counter_at(750), 250);
    }

    fn src(name: &str, priority: u8, period: f64, offset: f64, handler: f64) -> IrqSource {
        IrqSource { name: name.into(), priority, period_us: period, offset_us: offset, handler_us: handler, enabled: true }
    }

    #[test]
    fn a_lone_interrupt_starts_after_the_entry_time() {
        let trace = simulate_nvic(&[src("timer", 2, 1000.0, 100.0, 50.0)], 1000.0, 1.0, 1.0);
        assert!((trace.worst_latency_us(0).unwrap() - 1.0).abs() < 1e-6);
        let handler: f64 = trace.segments.iter().filter(|s| s.who == 1).map(|s| s.end_us - s.start_us).sum();
        assert!((handler - 52.0).abs() < 1e-6, "{handler}");
        assert!((trace.main_share - 0.948).abs() < 1e-6);
    }

    #[test]
    fn urgent_interrupts_preempt_and_less_urgent_ones_wait() {
        // "slow" (priority 3) runs 100..300. "fast" (priority 1) arrives at 150.
        let sources = [src("slow", 3, 10_000.0, 100.0, 200.0), src("fast", 1, 10_000.0, 150.0, 20.0)];
        let trace = simulate_nvic(&sources, 1000.0, 1.0, 1.0);
        assert!((trace.worst_latency_us(1).unwrap() - 1.0).abs() < 1e-6, "fast preempts at once");
        // Swap the priorities: now "fast" has to wait for "slow" to finish.
        let sources = [src("slow", 1, 10_000.0, 100.0, 200.0), src("fast", 3, 10_000.0, 150.0, 20.0)];
        let trace = simulate_nvic(&sources, 1000.0, 1.0, 1.0);
        let wait = trace.worst_latency_us(1).unwrap();
        assert!(wait > 150.0 && wait < 153.0, "{wait}");
        // Equal priority does not preempt either.
        let sources = [src("slow", 2, 10_000.0, 100.0, 200.0), src("fast", 2, 10_000.0, 150.0, 20.0)];
        assert!(simulate_nvic(&sources, 1000.0, 1.0, 1.0).worst_latency_us(1).unwrap() > 150.0);
    }

    #[test]
    fn requests_are_lost_when_the_handler_is_too_slow() {
        // A low-priority interrupt that fires three times while a long urgent handler runs:
        // only one request can wait, the others are lost.
        let sources = [src("hog", 0, 100.0, 0.0, 95.0), src("victim", 5, 30.0, 10.0, 2.0)];
        let trace = simulate_nvic(&sources, 5000.0, 1.0, 1.0);
        assert!(trace.lost(1) > 0);
        assert!(trace.main_share < 0.05, "{}", trace.main_share);
    }

    #[test]
    fn watchdog_resets_after_a_hang_and_not_before() {
        let w = Watchdog { timeout_ms: 100.0, loop_ms: 20.0, boot_ms: 50.0, hang_at_ms: Some(300.0), kick_from_timer: false };
        let tr = simulate_watchdog(&w, 1000.0);
        // Kicks at 70, 90 ... 290; stuck at 300; the last kick + 100 ms = 390.
        assert_eq!(tr.kicks.first(), Some(&70.0));
        assert_eq!(tr.resets, [390.0]);
        assert_eq!(tr.states[..4], [(FwState::Booting, 0.0, 50.0), (FwState::Running, 50.0, 300.0), (FwState::Hung, 300.0, 390.0), (FwState::Booting, 390.0, 440.0)]);
        assert!((tr.working_share - 0.81).abs() < 1e-9);
        // No hang, no reset.
        assert!(simulate_watchdog(&Watchdog { hang_at_ms: None, ..w }, 1000.0).resets.is_empty());
        // A loop slower than the timeout: reset, boot, reset ... and no work gets done past the first pass.
        let tr = simulate_watchdog(&Watchdog { loop_ms: 150.0, hang_at_ms: None, ..w }, 1000.0);
        assert_eq!(tr.resets, [150.0, 300.0, 450.0, 600.0, 750.0, 900.0]);
        assert!(tr.kicks.is_empty());
        // Kicking from a timer interrupt hides the hang for ever.
        let tr = simulate_watchdog(&Watchdog { kick_from_timer: true, ..w }, 1000.0);
        assert!(tr.resets.is_empty());
        assert_eq!(tr.states.last(), Some(&(FwState::Hung, 300.0, 1000.0)));
        assert!((tr.working_share - 0.25).abs() < 1e-9);
    }

    #[test]
    fn priority_inversion_and_its_cure() {
        let t = |name: &str, priority, arrive_ms, script: &[(f64, bool)]| LockTask { name: name.into(), priority, arrive_ms, script: script.to_vec() };
        let tasks = [
            t("low", 3, 0.0, &[(2.0, false), (6.0, true), (1.0, false)]),
            t("medium", 2, 7.0, &[(20.0, false)]),
            t("high", 1, 5.0, &[(1.0, false), (2.0, true), (1.0, false)]),
        ];
        let close = |a: Option<f64>, b: f64| (a.unwrap() - b).abs() < 1e-6;
        // Without inheritance "high" waits for all of "medium": 5..6 runs, blocked until
        // "low" can finish its critical section at 29, done at 32.
        let tr = simulate_lock(&tasks, false, 60.0);
        assert!(close(tr.finished_ms[2], 32.0), "{:?}", tr.finished_ms);
        assert!(close(tr.finished_ms[1], 27.0));
        assert!(tr.spans.iter().any(|s| s.0 == 2 && s.1 == LockState::Blocked && (s.2 - 6.0).abs() < 1e-6 && (s.3 - 29.0).abs() < 1e-6));
        // With inheritance "low" finishes its critical section at 9 and "high" is done at 12.
        let tr = simulate_lock(&tasks, true, 60.0);
        assert!(close(tr.finished_ms[2], 12.0), "{:?}", tr.finished_ms);
        assert!(close(tr.finished_ms[1], 32.0));
        assert!(close(tr.finished_ms[0], 33.0));
    }

    #[test]
    fn ram_layout_arithmetic() {
        let r = RamLayout { ram_bytes: 8192, globals_bytes: 1024, heap_bytes: 2048, call_depth: 8, frame_bytes: 64 };
        assert_eq!(r.stack_bytes(), 512);
        assert_eq!(r.free_bytes(), 4608);
        assert_eq!(r.heap_end(), 0x2000_0C00);
        assert_eq!(r.stack_top(), 0x2000_2000);
        assert_eq!(r.stack_pointer(), 0x2000_1E00);
        assert!(!r.collided());
        assert!(RamLayout { call_depth: 100, ..r }.collided());
    }

    #[test]
    fn clock_tree_frequencies() {
        // 8 MHz crystal / 1 x 12 = 96 MHz, / 2 = 48 MHz for the processor, / 2 = 24 MHz bus.
        let c = ClockTree { source_hz: 8e6, tolerance_ppm: 20.0, pll_on: true, pll_div: 1, pll_mul: 12, sys_div: 2, bus_div: 2 };
        assert_eq!((c.pll_hz(), c.cpu_hz(), c.bus_hz()), (96e6, 48e6, 24e6));
        assert_eq!(ClockTree { pll_on: false, sys_div: 1, ..c }.cpu_hz(), 8e6);
        // 20 ppm of a day is 1.728 s.
        assert!((c.drift_s_per_day() - 1.728).abs() < 1e-9);
        // 10-bit frames tolerate 0.5 / 9.5 = 5.26 % mismatch at the very most.
        assert!((uart_mismatch_limit(10) - 0.0526316).abs() < 1e-6);
    }

    #[test]
    fn dma_takes_the_load_off_the_processor() {
        // 1 Mbit/s = 100 000 bytes/s. 150 cycles each at 48 MHz = 31.25 % of the processor.
        let s = ByteStream { baud: 1e6, cpu_hz: 48e6, cycles_per_byte_irq: 150.0, block_bytes: 256, cycles_per_block_irq: 300.0, blocked_us: 0.0 };
        assert!((s.irq_load() - 0.3125).abs() < 1e-12);
        assert_eq!(s.byte_time_us(), 10.0);
        // With DMA: 390.6 blocks/s x 300 cycles = 0.24 %.
        assert!((s.dma_load() - 0.00244140625).abs() < 1e-12);
        assert!(!s.irq_loses_bytes());
        // A handler elsewhere that blocks for 20 µs costs bytes: one byte takes only 10 µs.
        assert!(ByteStream { blocked_us: 20.0, ..s }.irq_loses_bytes());
        // 8 MHz processor: 150 cycles = 18.75 µs, longer than a byte.
        assert!(ByteStream { cpu_hz: 8e6, ..s }.irq_loses_bytes());
    }

    #[test]
    fn firmware_update_outcomes() {
        let ok = UpdateScenario { two_slots: true, power_cut_while_writing: false, download_corrupted: false, new_version_faulty: false };
        let end = |s: UpdateScenario| firmware_update(s).last().unwrap().running;
        assert_eq!(end(ok), Some(2));
        // With two slots every mishap ends with the old version still running.
        assert_eq!(end(UpdateScenario { power_cut_while_writing: true, ..ok }), Some(1));
        assert_eq!(end(UpdateScenario { download_corrupted: true, ..ok }), Some(1));
        assert_eq!(end(UpdateScenario { new_version_faulty: true, ..ok }), Some(1));
        // Overwriting in place: fine when nothing goes wrong, dead otherwise.
        let in_place = UpdateScenario { two_slots: false, ..ok };
        assert_eq!(end(in_place), Some(2));
        assert_eq!(end(UpdateScenario { power_cut_while_writing: true, ..in_place }), None);
        assert_eq!(end(UpdateScenario { download_corrupted: true, ..in_place }), None);
        assert_eq!(end(UpdateScenario { new_version_faulty: true, ..in_place }), None);
    }
}
