//! What Claude models cost per million tokens, so a cost can be estimated from token counts.
//!
//! These are the Anthropic API list prices, last checked on 2026-10-30 from the model table in the
//! Claude API reference. They are an estimate of API cost: a subscription is not billed per token.
//! A model that is not here is counted in tokens only, never guessed.

/// Dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    /// Writing the cache with the default 5-minute lifetime.
    pub cache_write_5m: f64,
    /// Writing the cache with the 1-hour lifetime.
    pub cache_write_1h: f64,
}

impl Price {
    /// Cache reads at `read_factor` of the input price; writes at 1.25 and 2 times.
    const fn new(input: f64, output: f64, read_factor: f64) -> Self {
        Price {
            input,
            output,
            cache_read: input * read_factor,
            cache_write_5m: input * 1.25,
            cache_write_1h: input * 2.0,
        }
    }
}

/// Claude Haiku 5.5 costs five times more once a prompt passes this many tokens.
const HAIKU_5_5_LONG_PROMPT: u64 = 100_000;

/// The price of one request. `prompt_tokens` is everything sent in: plain input, cache reads and
/// cache writes.
pub fn price_of(model: &str, prompt_tokens: u64) -> Option<Price> {
    // Longest prefixes first, so a dated or suffixed id still finds its model.
    let table: &[(&str, Price)] = &[
        ("claude-fable-5-1", Price::new(10.0, 50.0, 0.025)),
        ("claude-mythos-5-1", Price::new(10.0, 50.0, 0.025)),
        ("claude-fable-5", Price::new(10.0, 50.0, 0.1)),
        ("claude-mythos-5", Price::new(10.0, 50.0, 0.1)),
        ("claude-opus-5-5", Price::new(4.0, 20.0, 0.05)),
        ("claude-opus-5", Price::new(5.0, 25.0, 0.1)),
        ("claude-opus-4-8", Price::new(5.0, 25.0, 0.1)),
        ("claude-opus-4-7", Price::new(5.0, 25.0, 0.1)),
        ("claude-opus-4-6", Price::new(5.0, 25.0, 0.1)),
        ("claude-opus-4-5", Price::new(5.0, 25.0, 0.1)),
        ("claude-sonnet-5-5", Price::new(2.0, 10.0, 0.1)),
        ("claude-sonnet-5", Price::new(2.0, 10.0, 0.1)),
        ("claude-sonnet-4-6", Price::new(3.0, 15.0, 0.1)),
        ("claude-sonnet-4-5", Price::new(3.0, 15.0, 0.1)),
        ("claude-haiku-4-5", Price::new(1.0, 5.0, 0.1)),
    ];
    if model.starts_with("claude-haiku-5-5") {
        return Some(if prompt_tokens > HAIKU_5_5_LONG_PROMPT {
            Price::new(0.5, 2.5, 0.1)
        } else {
            Price::new(0.1, 0.5, 0.1)
        });
    }
    table
        .iter()
        .find(|(prefix, _)| model.starts_with(prefix))
        .map(|(_, price)| *price)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_finds_its_price_by_name_even_with_a_date_after_it() {
        let sonnet = price_of("claude-sonnet-5-5", 0).unwrap();
        assert_eq!((sonnet.input, sonnet.output), (2.0, 10.0));
        assert!((sonnet.cache_read - 0.2).abs() < 1e-9);
        assert!((sonnet.cache_write_5m - 2.5).abs() < 1e-9);
        assert!((sonnet.cache_write_1h - 4.0).abs() < 1e-9);
        assert_eq!(price_of("claude-haiku-4-5-20251001", 0).unwrap().input, 1.0);
    }

    #[test]
    fn the_newest_model_is_not_mistaken_for_the_older_one_it_starts_with() {
        assert_eq!(price_of("claude-opus-5-5", 0).unwrap().input, 4.0);
        assert_eq!(price_of("claude-opus-5", 0).unwrap().input, 5.0);
        assert!((price_of("claude-fable-5-1", 0).unwrap().cache_read - 0.25).abs() < 1e-9);
        assert!((price_of("claude-fable-5", 0).unwrap().cache_read - 1.0).abs() < 1e-9);
        assert!((price_of("claude-opus-5-5", 0).unwrap().cache_read - 0.2).abs() < 1e-9);
    }

    #[test]
    fn a_long_prompt_costs_more_on_haiku_5_5() {
        assert_eq!(price_of("claude-haiku-5-5", 1_000).unwrap().input, 0.1);
        assert_eq!(price_of("claude-haiku-5-5", 100_001).unwrap().input, 0.5);
    }

    #[test]
    fn an_unknown_model_has_no_price_and_is_never_guessed() {
        assert_eq!(price_of("<synthetic>", 0), None);
        assert_eq!(price_of("claude-something-new-9", 0), None);
    }
}
