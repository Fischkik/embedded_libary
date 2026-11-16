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

#[cfg(test)]
mod tests {
    use super::*;

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
}
