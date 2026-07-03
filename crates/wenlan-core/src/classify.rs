// SPDX-License-Identifier: Apache-2.0
//! Memory classification via the on-device LLM engine.
//!
//! Wraps [`crate::engine::LlmEngine`] with classification-specific prompts and
//! response parsers. Covers single-memory classification, batch classification,
//! type-only fallback classification, and profile sub-classification (alias
//! "profile" -> identity / preference / goal).

use crate::engine::{extract_json_array, LlmEngine, CTX_SIZE};

use wenlan_types::MemoryType;

/// Result of classifying a single memory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassificationResult {
    pub memory_type: String,
    pub space: Option<String>,
    pub tags: Vec<String>,
    /// Quality signal: "low", "medium", "high", or None (default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// T8 salience prior: per-memory importance rating 1-10 (LLM-assigned at
    /// write time), or None when absent/malformed. NEVER defaults to a number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub importance: Option<u8>,
}

impl Default for ClassificationResult {
    fn default() -> Self {
        Self {
            memory_type: "fact".to_string(),
            space: None,
            tags: Vec::new(),
            quality: None,
            importance: None,
        }
    }
}

/// Parse a batch classification response from LLM output.
/// Extracts JSON array, validates each entry, falls back to defaults for invalid entries,
/// and pads with defaults if the array is shorter than expected.
pub fn parse_classification_response(
    raw: &str,
    expected_count: usize,
) -> Vec<ClassificationResult> {
    let json_str = match extract_json_array(raw) {
        Some(s) => s,
        None => return vec![ClassificationResult::default(); expected_count],
    };

    let entries: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return vec![ClassificationResult::default(); expected_count],
    };

    let mut results: Vec<ClassificationResult> = entries
        .iter()
        .map(|entry| {
            let memory_type = entry
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase())
                .filter(|s| MemoryType::all_values().contains(&s.as_str()))
                .unwrap_or_else(|| "fact".to_string());

            let space = entry
                .get("domain")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            let tags = entry
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            ClassificationResult {
                memory_type,
                space,
                tags,
                quality: None,
                importance: None,
            }
        })
        .collect();

    // Pad with defaults if fewer results than expected
    while results.len() < expected_count {
        results.push(ClassificationResult::default());
    }

    results
}

/// Parse LLM output for profile sub-classification. Fallback: "identity" (Protected tier).
/// Profile alias resolves to identity | preference (Goal deprecated -> Identity).
pub fn parse_profile_subtype(raw: &str) -> &'static str {
    let first = raw.split_whitespace().next().unwrap_or("").to_lowercase();
    match first.as_str() {
        "identity" => "identity",
        "preference" => "preference",
        // Deprecated: legacy LLM output still says "goal" -> fold to identity.
        "goal" => "identity",
        _ => "identity", // safest default -- Protected tier
    }
}

/// Classify a "profile" alias into identity / preference using the on-device engine.
///
/// Runs synchronously on the engine's worker thread. Caller is responsible for
/// running this inside `spawn_blocking` if invoking from an async context, since
/// [`LlmEngine::run_inference`] is blocking. Falls back to "identity" (Protected
/// tier) on any failure.
pub fn classify_profile_subtype_llm(engine: &LlmEngine, content: &str) -> String {
    let truncated: String = content.chars().take(500).collect();

    let system_prompt = crate::prompts::defaults::CLASSIFY_PROFILE_SUBTYPE;
    let prompt = format!(
        "<|im_start|>system\n\
         {system_prompt}\n\
         <|im_end|>\n\
         <|im_start|>user\n\
         {truncated}\n\
         <|im_end|>\n\
         <|im_start|>assistant\n",
    );

    match engine.run_inference(&prompt, 16, 0.1, crate::engine::CTX_SIZE, Some("classify")) {
        Some(output) => parse_profile_subtype(&output).to_string(),
        None => {
            log::warn!("[classify] profile sub-classify failed");
            "identity".to_string()
        }
    }
}

/// Async variant of [`classify_profile_subtype_llm`] that works with any
/// [`LlmProvider`]. Used by the app/server layers where the LLM is only
/// available through the provider trait (not a raw `LlmEngine`).
pub async fn classify_profile_subtype_via_provider(
    provider: &dyn crate::llm_provider::LlmProvider,
    content: &str,
) -> String {
    let truncated: String = content.chars().take(500).collect();

    let request = crate::llm_provider::LlmRequest {
        system_prompt: Some(crate::prompts::defaults::CLASSIFY_PROFILE_SUBTYPE.to_string()),
        user_prompt: truncated,
        max_tokens: 16,
        temperature: 0.1,
        label: None,
        timeout_secs: None,
    };

    match provider.generate(request).await {
        Ok(output) => parse_profile_subtype(&output).to_string(),
        Err(e) => {
            log::warn!("[classify] profile sub-classify via provider failed: {e}");
            "identity".to_string()
        }
    }
}

#[allow(dead_code)] // Full classification API -- only classify_memories_batch wired through OnDeviceProvider currently
impl LlmEngine {
    /// Classify a batch of memories into types, domains, and tags using on-device LLM.
    /// Returns one `ClassificationResult` per input memory. Falls back to defaults on failure.
    pub fn classify_memories_batch(&self, memories: &[String]) -> Vec<ClassificationResult> {
        if memories.is_empty() {
            return Vec::new();
        }

        let numbered = memories
            .iter()
            .enumerate()
            .map(|(i, m)| format!("{}. {}", i + 1, m))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "<|im_start|>system\n\
             {sys}\n\
             <|im_end|>\n\
             <|im_start|>user\n\
             {numbered}\n\
             <|im_end|>\n\
             <|im_start|>assistant\n",
            sys = self.prompts().batch_classify,
        );

        match self.run_inference(&prompt, 2048, 0.3, CTX_SIZE, Some("classify")) {
            Some(response) => parse_classification_response(&response, memories.len()),
            None => vec![ClassificationResult::default(); memories.len()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_classification_response_valid() {
        let json = r#"[{"i":1,"type":"identity","domain":"work","tags":["engineer"]},{"i":2,"type":"preference","domain":"technology","tags":["dark mode"]}]"#;
        let results = parse_classification_response(json, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].memory_type, "identity");
        assert_eq!(results[0].space, Some("work".to_string()));
        assert_eq!(results[1].memory_type, "preference");
    }

    #[test]
    fn test_parse_classification_response_malformed_entry() {
        let json =
            r#"[{"i":1,"type":"identity","domain":"work","tags":[]},{"i":2,"type":"INVALID"}]"#;
        let results = parse_classification_response(json, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].memory_type, "identity");
        assert_eq!(results[1].memory_type, "fact"); // invalid type falls back
    }

    #[test]
    fn test_parse_classification_response_total_garbage() {
        let json = "this is not json at all";
        let results = parse_classification_response(json, 3);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.memory_type == "fact"));
    }

    #[test]
    fn test_parse_classification_response_wrong_count() {
        let json = r#"[{"i":1,"type":"identity","domain":"work","tags":[]}]"#;
        let results = parse_classification_response(json, 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].memory_type, "identity");
        assert_eq!(results[1].memory_type, "fact"); // padded
    }

    #[test]
    fn test_parse_classification_response_with_surrounding_text() {
        let json = r#"Here are the results: [{"i":1,"type":"fact","domain":"work","tags":[]}] Hope that helps!"#;
        let results = parse_classification_response(json, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_type, "fact");
    }

    #[test]
    fn test_parse_profile_subtype() {
        assert_eq!(parse_profile_subtype("identity"), "identity");
        assert_eq!(
            parse_profile_subtype("preference\nsome extra"),
            "preference"
        );
        // Deprecated: "goal" folds to identity (post-taxonomy refactor).
        assert_eq!(parse_profile_subtype("GOAL"), "identity");
        assert_eq!(parse_profile_subtype("bogus"), "identity"); // fallback
        assert_eq!(parse_profile_subtype(""), "identity"); // fallback
    }

    #[test]
    fn test_classify_prompt_contains_all_types() {
        // Drift guard: the compiled-in classify prompt must list every
        // canonical type so the LLM never emits a value our parser rejects.
        // Derive the haystack from the actual prompt const rather than a
        // hand-typed string, otherwise the test rots in parallel with the
        // prompt itself.
        use crate::prompts::defaults::CLASSIFY_MEMORY_QUALITY;
        for val in MemoryType::all_values() {
            assert!(
                CLASSIFY_MEMORY_QUALITY.contains(val),
                "CLASSIFY_MEMORY_QUALITY prompt must include canonical type \"{val}\"",
            );
        }
    }

    #[test]
    fn test_classify_prompts_omit_legacy_goal() {
        // "goal" is folded to Identity by MemoryType::FromStr — it must not
        // appear in any LLM-facing prompt or the model will keep emitting it.
        use crate::prompts::defaults::{
            BATCH_CLASSIFY, CLASSIFY_MEMORY, CLASSIFY_MEMORY_QUALITY,
            CLASSIFY_MEMORY_QUALITY_STRICT, CLASSIFY_PROFILE_SUBTYPE,
        };
        for (label, prompt) in [
            ("CLASSIFY_MEMORY", CLASSIFY_MEMORY),
            ("CLASSIFY_MEMORY_QUALITY", CLASSIFY_MEMORY_QUALITY),
            (
                "CLASSIFY_MEMORY_QUALITY_STRICT",
                CLASSIFY_MEMORY_QUALITY_STRICT,
            ),
            ("CLASSIFY_PROFILE_SUBTYPE", CLASSIFY_PROFILE_SUBTYPE),
            ("BATCH_CLASSIFY", BATCH_CLASSIFY),
        ] {
            let has_goal = prompt
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|tok| tok == "goal");
            assert!(
                !has_goal,
                "{label} prompt still advertises legacy \"goal\" token",
            );
        }
    }
}
