// SPDX-License-Identifier: AGPL-3.0-only
/// Unified trigger event enum. All input types flow through a single
/// `tokio::sync::mpsc::channel<TriggerEvent>` for serial processing.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TriggerEvent {
    /// Global keyboard shortcut (Cmd+Shift+M).
    /// Forces OCR + intent classification.
    ManualHotkey,

    /// User typed a thought into quick-capture UI.
    /// Bypasses vision entirely — zero compute.
    QuickThought { text: String },
}
