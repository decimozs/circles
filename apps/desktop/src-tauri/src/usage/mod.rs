//! How much Claude Code has been used: tokens and an estimate of what they would cost on the API.
//!
//! The numbers come from Claude Code's own transcripts (`scan`), so they cover every conversation
//! on this Mac, including those that were not started from Circles. Cost is estimated from list
//! prices (`prices`) and is not what a subscription is billed.

pub mod prices;
pub mod scan;

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use ts_rs::TS;

use self::scan::Record;

/// Token counts over a range.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct UsageTotals {
    /// Everything the model read and wrote.
    #[ts(type = "number")]
    pub processed: u64,
    /// Input served from the cache.
    #[ts(type = "number")]
    pub cached_input: u64,
    /// Input that was not served from the cache, including what was written to it.
    #[ts(type = "number")]
    pub uncached_input: u64,
    #[ts(type = "number")]
    pub output: u64,
    /// What the cache reads would have cost as plain input, less what they cost.
    pub cache_savings_usd: f64,
    pub cost_usd: f64,
    pub cost_input_usd: f64,
    pub cost_cache_read_usd: f64,
    pub cost_cache_write_usd: f64,
    pub cost_output_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct UsageDay {
    /// `YYYY-MM-DD`, in UTC.
    pub day: String,
    pub cost_usd: f64,
    #[ts(type = "number")]
    pub tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct UsageModel {
    pub model: String,
    pub cost_usd: f64,
    #[ts(type = "number")]
    pub tokens: u64,
    pub messages: u32,
    /// False when Circles has no price for the model: its tokens are counted, its cost is not.
    pub priced: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct UsageSummary {
    pub sessions: u32,
    pub messages: u32,
    pub totals: UsageTotals,
    /// Oldest first. Days with no use are not listed.
    pub by_day: Vec<UsageDay>,
    /// Most expensive first.
    pub by_model: Vec<UsageModel>,
    /// The Claude folders that were read.
    pub folders: Vec<String>,
    /// Transcript lines that looked like usage but could not be read.
    pub skipped_lines: u32,
}

/// Adds up `records` that are at or after `since` (unix seconds) and, when `sessions` is given,
/// belong to one of those Claude sessions.
pub fn summarize(
    records: &[Record],
    since: Option<i64>,
    sessions: Option<&HashSet<String>>,
) -> UsageSummary {
    let mut summary = UsageSummary::default();
    let mut days: BTreeMap<&str, (f64, u64)> = BTreeMap::new();
    let mut models: BTreeMap<&str, UsageModel> = BTreeMap::new();
    let mut seen_sessions: HashSet<&str> = HashSet::new();

    for r in records {
        if since.is_some_and(|s| r.at < s) {
            continue;
        }
        if sessions.is_some_and(|set| !set.contains(&r.session)) {
            continue;
        }
        let written = r.cache_write_5m + r.cache_write_1h;
        let tokens = r.input + r.output + r.cache_read + written;
        let price = prices::price_of(&r.model, r.input + r.cache_read + written);
        let per = |count: u64, rate: f64| count as f64 * rate / 1_000_000.0;
        let cost = price.map_or(0.0, |p| {
            let (input, read, write, output) = (
                per(r.input, p.input),
                per(r.cache_read, p.cache_read),
                per(r.cache_write_5m, p.cache_write_5m) + per(r.cache_write_1h, p.cache_write_1h),
                per(r.output, p.output),
            );
            let t = &mut summary.totals;
            t.cost_input_usd += input;
            t.cost_cache_read_usd += read;
            t.cost_cache_write_usd += write;
            t.cost_output_usd += output;
            t.cache_savings_usd += per(r.cache_read, p.input - p.cache_read);
            input + read + write + output
        });

        summary.messages += 1;
        seen_sessions.insert(&r.session);
        let t = &mut summary.totals;
        t.processed += tokens;
        t.cached_input += r.cache_read;
        t.uncached_input += r.input + written;
        t.output += r.output;
        t.cost_usd += cost;

        let day = days.entry(&r.day).or_default();
        day.0 += cost;
        day.1 += tokens;
        let model = models.entry(&r.model).or_insert_with(|| UsageModel {
            model: r.model.clone(),
            cost_usd: 0.0,
            tokens: 0,
            messages: 0,
            priced: price.is_some(),
        });
        model.cost_usd += cost;
        model.tokens += tokens;
        model.messages += 1;
    }

    summary.sessions = seen_sessions.len() as u32;
    summary.by_day = days
        .into_iter()
        .map(|(day, (cost_usd, tokens))| UsageDay {
            day: day.to_string(),
            cost_usd,
            tokens,
        })
        .collect();
    let mut by_model: Vec<UsageModel> = models.into_values().collect();
    by_model.sort_by(|a, b| {
        b.cost_usd
            .total_cmp(&a.cost_usd)
            .then(b.tokens.cmp(&a.tokens))
            .then(a.model.cmp(&b.model))
    });
    summary.by_model = by_model;
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, model: &str, at: i64, day: &str, session: &str) -> Record {
        Record {
            id: id.into(),
            model: model.into(),
            at,
            day: day.into(),
            session: session.into(),
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 0,
        }
    }

    #[test]
    fn tokens_and_cost_add_up_by_type() {
        let s = summarize(
            &[rec("a", "claude-sonnet-5-5", 10, "2026-10-01", "s")],
            None,
            None,
        );
        let t = &s.totals;
        assert_eq!(t.processed, 4_000_000);
        assert_eq!(
            (t.cached_input, t.uncached_input, t.output),
            (1_000_000, 2_000_000, 1_000_000)
        );
        // Sonnet 5.5: input 2, output 10, cache read 0.2, 5 minute write 2.5, per million.
        assert!((t.cost_input_usd - 2.0).abs() < 1e-9);
        assert!((t.cost_cache_read_usd - 0.2).abs() < 1e-9);
        assert!((t.cost_cache_write_usd - 2.5).abs() < 1e-9);
        assert!((t.cost_output_usd - 10.0).abs() < 1e-9);
        assert!((t.cost_usd - 14.7).abs() < 1e-9);
        assert!((t.cache_savings_usd - 1.8).abs() < 1e-9);
        assert_eq!((s.sessions, s.messages), (1, 1));
    }

    #[test]
    fn the_range_and_the_session_filter_leave_records_out() {
        let records = [
            rec("a", "claude-sonnet-5-5", 100, "2026-10-01", "one"),
            rec("b", "claude-sonnet-5-5", 200, "2026-10-02", "two"),
        ];
        assert_eq!(summarize(&records, Some(150), None).messages, 1);
        let only: HashSet<String> = ["one".to_string()].into();
        let s = summarize(&records, None, Some(&only));
        assert_eq!((s.messages, s.sessions), (1, 1));
        assert_eq!(s.by_day[0].day, "2026-10-01");
    }

    #[test]
    fn days_run_oldest_first_and_models_most_expensive_first() {
        let records = [
            rec("a", "claude-haiku-4-5", 1, "2026-10-02", "s"),
            rec("b", "claude-opus-5-5", 2, "2026-10-01", "s"),
            rec("c", "claude-opus-5-5", 3, "2026-10-02", "s"),
        ];
        let s = summarize(&records, None, None);
        assert_eq!(
            s.by_day.iter().map(|d| d.day.as_str()).collect::<Vec<_>>(),
            ["2026-10-01", "2026-10-02"]
        );
        assert_eq!(s.by_model[0].model, "claude-opus-5-5");
        assert_eq!(s.by_model[0].messages, 2);
    }

    #[test]
    fn a_model_without_a_price_adds_tokens_but_no_cost() {
        let s = summarize(
            &[rec("a", "claude-future-9", 1, "2026-10-01", "s")],
            None,
            None,
        );
        assert_eq!(s.totals.processed, 4_000_000);
        assert_eq!(s.totals.cost_usd, 0.0);
        assert!(!s.by_model[0].priced);
    }

    /// Reads the real transcripts on this Mac:
    /// `cargo test --lib -- --ignored --nocapture real_transcripts`
    #[test]
    #[ignore = "reads the real Claude Code transcripts of this Mac"]
    fn real_transcripts_add_up() {
        let home = dirs::home_dir();
        let dirs = scan::config_dirs(home.as_deref(), None);
        let started = std::time::Instant::now();
        let scanned = scan::scan(&dirs);
        let first = started.elapsed();
        let summary = summarize(&scanned.records, None, None);
        let again = std::time::Instant::now();
        let _ = scan::scan(&dirs);
        println!(
            "folders {dirs:?}\nfirst scan {first:?}, second {:?}\nmessages {} sessions {} skipped {}\ncost ${:.2} processed {} savings ${:.2}",
            again.elapsed(),
            summary.messages,
            summary.sessions,
            scanned.skipped,
            summary.totals.cost_usd,
            summary.totals.processed,
            summary.totals.cache_savings_usd
        );
        for m in &summary.by_model {
            println!(
                "{} ${:.2} {} tokens, {} messages, priced {}",
                m.model, m.cost_usd, m.tokens, m.messages, m.priced
            );
        }
        assert!(summary.messages > 0);
    }
}
