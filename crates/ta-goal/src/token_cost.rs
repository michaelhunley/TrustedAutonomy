// token_cost.rs — Model-keyed rate table for computing LLM API cost from token usage.
//
// Rate table is updated at release time — not fetched at runtime.
// For non-Claude agents, costs remain 0 with `cost_estimated = false`.
//
// Rates as of 2026-04: https://www.anthropic.com/pricing

/// Input/output price pair in USD per 1 million tokens.
#[derive(Debug, Clone, Copy)]
pub struct ModelRate {
    /// USD per 1M input tokens.
    pub input_per_m: f64,
    /// USD per 1M output tokens.
    pub output_per_m: f64,
}

/// Resolve the ModelRate for a model identifier.
///
/// Matching is case-insensitive prefix/substring matching on the model string
/// so `claude-sonnet-4-6-20250514` and `claude-sonnet-4-6` both resolve to Sonnet 4.6.
///
/// Returns `None` for unknown / non-Claude models (Ollama, Codex, etc.).
pub fn rate_for_model(model: &str) -> Option<ModelRate> {
    let lower = model.to_lowercase();
    // Claude 4 family — check most-specific first.
    if lower.contains("claude-opus-4-6") || lower.contains("claude-opus-4.6") {
        return Some(ModelRate {
            input_per_m: 15.0,
            output_per_m: 75.0,
        });
    }
    if lower.contains("claude-opus-4-5") || lower.contains("claude-opus-4.5") {
        return Some(ModelRate {
            input_per_m: 15.0,
            output_per_m: 75.0,
        });
    }
    if lower.contains("claude-opus-4") {
        return Some(ModelRate {
            input_per_m: 15.0,
            output_per_m: 75.0,
        });
    }
    if lower.contains("claude-sonnet-4-6") || lower.contains("claude-sonnet-4.6") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    if lower.contains("claude-sonnet-4-5") || lower.contains("claude-sonnet-4.5") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    if lower.contains("claude-sonnet-4") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    if lower.contains("claude-haiku-4-5") || lower.contains("claude-haiku-4.5") {
        return Some(ModelRate {
            input_per_m: 0.80,
            output_per_m: 4.0,
        });
    }
    if lower.contains("claude-haiku-4") {
        return Some(ModelRate {
            input_per_m: 0.80,
            output_per_m: 4.0,
        });
    }
    // Claude 3.x legacy family.
    if lower.contains("claude-opus-3") {
        return Some(ModelRate {
            input_per_m: 15.0,
            output_per_m: 75.0,
        });
    }
    if lower.contains("claude-sonnet-3-7") || lower.contains("claude-sonnet-3.7") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    if lower.contains("claude-sonnet-3-5") || lower.contains("claude-sonnet-3.5") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    if lower.contains("claude-sonnet-3") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    if lower.contains("claude-haiku-3-5") || lower.contains("claude-haiku-3.5") {
        return Some(ModelRate {
            input_per_m: 0.80,
            output_per_m: 4.0,
        });
    }
    if lower.contains("claude-haiku-3") {
        return Some(ModelRate {
            input_per_m: 0.25,
            output_per_m: 1.25,
        });
    }
    // Generic Claude fallback (unknown version).
    if lower.contains("claude") {
        return Some(ModelRate {
            input_per_m: 3.0,
            output_per_m: 15.0,
        });
    }
    // Not a Claude model — Ollama, Codex, unknown agents.
    None
}

/// Compute USD cost for a given model and token counts.
///
/// Returns `(cost_usd, cost_estimated)` where:
/// - `cost_estimated = true` if the rate was found (Claude models)
/// - `cost_estimated = false` if the model is unknown (Ollama, Codex)
pub fn compute_cost(model: &str, input_tokens: u64, output_tokens: u64) -> (f64, bool) {
    match rate_for_model(model) {
        Some(rate) => {
            let cost = (input_tokens as f64 / 1_000_000.0) * rate.input_per_m
                + (output_tokens as f64 / 1_000_000.0) * rate.output_per_m;
            (cost, true)
        }
        None => (0.0, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonnet_46_rate_resolves() {
        let rate = rate_for_model("claude-sonnet-4-6").unwrap();
        assert_eq!(rate.input_per_m, 3.0);
        assert_eq!(rate.output_per_m, 15.0);
    }

    #[test]
    fn sonnet_46_with_date_suffix() {
        let rate = rate_for_model("claude-sonnet-4-6-20250514").unwrap();
        assert_eq!(rate.input_per_m, 3.0);
    }

    #[test]
    fn opus_46_rate_resolves() {
        let rate = rate_for_model("claude-opus-4-6").unwrap();
        assert_eq!(rate.input_per_m, 15.0);
        assert_eq!(rate.output_per_m, 75.0);
    }

    #[test]
    fn haiku_45_rate_resolves() {
        let rate = rate_for_model("claude-haiku-4-5-20251001").unwrap();
        assert_eq!(rate.input_per_m, 0.80);
        assert_eq!(rate.output_per_m, 4.0);
    }

    #[test]
    fn ollama_returns_none() {
        assert!(rate_for_model("qwen3.5:9b").is_none());
        assert!(rate_for_model("llama3.3:70b").is_none());
    }

    #[test]
    fn compute_cost_sonnet_1m_tokens() {
        // 1M input + 1M output at Sonnet 4.6 rates = $3 + $15 = $18
        let (cost, estimated) = compute_cost("claude-sonnet-4-6", 1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.001);
        assert!(estimated);
    }

    #[test]
    fn compute_cost_ollama_is_zero_and_not_estimated() {
        let (cost, estimated) = compute_cost("qwen3.5:9b", 50_000, 10_000);
        assert_eq!(cost, 0.0);
        assert!(!estimated);
    }

    #[test]
    fn compute_cost_zero_tokens() {
        let (cost, _) = compute_cost("claude-sonnet-4-6", 0, 0);
        assert_eq!(cost, 0.0);
    }
}
