//! Port of `src/client/hud/layers/lib/GoldRateTracker.ts` — the per-player
//! rolling gold-rate bookkeeping behind the leaderboard income columns.
//!
//! Time is GAME TICKS (10/s), never wall time: a frozen sim clock yields 0.
//! Faithfulness notes (quirk list):
//!
//! * `history` is a JS `Map<number, Sample[]>` — insertion order is the
//!   first-record order of each smallID (observable through the dump); the
//!   array values are push-ordered snapshots `{...sample, tick}` (income,
//!   trade, train, piracy, tick key order).
//! * `record` eviction is a `while (samples[0].tick < cutoff) shift()` loop
//!   with `cutoff = tick - WINDOW_TICKS` — STRICT `<`: a sample exactly at
//!   the cutoff SURVIVES. The loop reads `samples[0].tick` each pass, so an
//!   empty array short-circuits on `length > 0`.
//! * The hard cap `if (samples.length > MAX_SAMPLES) splice(0, len -
//!   MAX_SAMPLES)` is STRICT `>`: exactly 240 samples never splices.
//! * `rate`: `!samples || samples.length < 2 → 0`; else the two-point slope
//!   `dtMin = (last.tick - first.tick) / TICKS_PER_MINUTE` with the `dtMin
//!   <= 0 → 0` gate (a frozen clock or a backwards pair yields 0, never a
//!   divide). All values are f64 (JS `number`), including the tick.
//! * `forget` is a `Map#delete` (removes the key, its array with it);
//!   `resetAll` is `Map#clear`.
//! * `TICKS_PER_MINUTE = 600`, `WINDOW_TICKS = 1200`, `MAX_SAMPLES = 240`.

use crate::desync_detector::NumMap;

/// `TICKS_PER_MINUTE` — the sim runs at 10 ticks/second.
pub const TICKS_PER_MINUTE: f64 = 600.0;
/// `WINDOW_TICKS` — 2 in-game minutes.
pub const WINDOW_TICKS: f64 = 1200.0;
/// `MAX_SAMPLES` — the hard memory cap per player.
pub const MAX_SAMPLES: usize = 240;

/// One `Sample` (`{...sample, tick}` — key order income,trade,train,piracy,tick).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sample {
    pub income: f64,
    pub trade: f64,
    pub train: f64,
    pub piracy: f64,
    pub tick: f64,
}

/// The ported `GoldRateTracker` (the module singleton `goldRateTracker` is
/// one instance of this).
#[derive(Debug, Default)]
pub struct GoldRateTracker {
    history: NumMap<Vec<Sample>>,
}

impl GoldRateTracker {
    /// `record(smallID, sample, tick)`.
    pub fn record(&mut self, small_id: f64, sample: Sample, tick: f64) {
        let mut s = sample;
        s.tick = tick;
        let samples = self.history.get_mut_or_default(small_id);
        samples.push(s);
        // Evict samples outside the in-game-time window (STRICT `<` cutoff).
        let cutoff = tick - WINDOW_TICKS;
        while samples.first().is_some_and(|first| first.tick < cutoff) {
            samples.remove(0);
        }
        // Hard cap to bound memory (STRICT `>`).
        if samples.len() > MAX_SAMPLES {
            let drop = samples.len() - MAX_SAMPLES;
            samples.drain(..drop);
        }
    }

    /// `forget(smallID)`.
    pub fn forget(&mut self, small_id: f64) {
        self.history.delete(small_id);
    }

    /// `resetAll()`.
    pub fn reset_all(&mut self) {
        self.history.clear();
    }

    /// The private two-point slope `rate(smallID, pick)`.
    fn rate(&self, small_id: f64, pick: fn(&Sample) -> f64) -> f64 {
        let Some(samples) = self.history.get(small_id) else {
            return 0.0;
        };
        if samples.len() < 2 {
            return 0.0;
        }
        let first = samples[0];
        let last = samples[samples.len() - 1];
        let dt_min = (last.tick - first.tick) / TICKS_PER_MINUTE;
        if dt_min <= 0.0 {
            return 0.0;
        }
        (pick(&last) - pick(&first)) / dt_min
    }

    pub fn gold_income_per_min(&self, small_id: f64) -> f64 {
        self.rate(small_id, |s| s.income)
    }
    pub fn ship_trade_gold_per_min(&self, small_id: f64) -> f64 {
        self.rate(small_id, |s| s.trade)
    }
    pub fn train_trade_gold_per_min(&self, small_id: f64) -> f64 {
        self.rate(small_id, |s| s.train)
    }
    pub fn piracy_gold_per_min(&self, small_id: f64) -> f64 {
        self.rate(small_id, |s| s.piracy)
    }
}

/// The capture harness: an op stream over one tracker instance.
#[derive(Debug, Default)]
pub struct RigHarness {
    tracker: GoldRateTracker,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 record `[smallID, income, trade, train, piracy, tick]` -> `[0]`;
    /// 2 forget `[smallID]` -> `[0]`;
    /// 3 resetAll -> `[0]`;
    /// 4 rate `[smallID, pick 0=income,1=trade,2=train,3=piracy]` -> `[f64]`;
    /// 5 dump -> `[n, (smallID, m, (income,trade,train,piracy,tick)*m)*n]`
    ///   (history Map in insertion order, each sample array in push order).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let small_id = args[i];
                i += 1;
                let sample = Sample {
                    income: args[i],
                    trade: args[i + 1],
                    train: args[i + 2],
                    piracy: args[i + 3],
                    tick: 0.0,
                };
                i += 4;
                let tick = args[i];
                self.tracker.record(small_id, sample, tick);
                vec![0.0]
            }
            2 => {
                self.tracker.forget(args[i]);
                vec![0.0]
            }
            3 => {
                self.tracker.reset_all();
                vec![0.0]
            }
            4 => {
                let small_id = args[i];
                i += 1;
                let v = match args[i] as u8 {
                    0 => self.tracker.gold_income_per_min(small_id),
                    1 => self.tracker.ship_trade_gold_per_min(small_id),
                    2 => self.tracker.train_trade_gold_per_min(small_id),
                    _ => self.tracker.piracy_gold_per_min(small_id),
                };
                vec![v]
            }
            5 => {
                let mut out = Vec::new();
                out.push(self.tracker.history.len() as f64);
                for (sid, samples) in self.tracker.history.iter() {
                    out.push(sid);
                    out.push(samples.len() as f64);
                    for s in samples {
                        out.extend_from_slice(&[s.income, s.trade, s.train, s.piracy, s.tick]);
                    }
                }
                out
            }
            k => unreachable!("gold_rate_tracker harness: unknown op kind {k}"),
        }
    }
}
