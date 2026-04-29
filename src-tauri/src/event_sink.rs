//! Abstraction over `tauri::AppHandle::emit` so the orchestrators can be driven by
//! tests with a capturing sink. Production wraps `AppHandle`; tests use `CapturingSink`.
//!
//! The wire payload (a `serde_json::Value` named `chat_event`) is identical
//! regardless of sink, which is what `event_payload_compat` pins.

use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

/// Anything that can deliver a `chat_event` payload to the frontend (or a test buffer).
pub trait EventSink: Send + Sync {
    fn emit_chat_event(&self, payload: Value);
}

/// Production sink: emits via `tauri::AppHandle::emit("chat_event", _)`.
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit_chat_event(&self, payload: Value) {
        let _ = self.app.emit("chat_event", payload);
    }
}

/// Test sink: stores every emitted payload in insertion order.
#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct CapturingSink {
    events: Arc<Mutex<Vec<Value>>>,
}

#[allow(dead_code)]
impl CapturingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.events.lock().unwrap().clone()
    }
}

impl EventSink for CapturingSink {
    fn emit_chat_event(&self, payload: Value) {
        self.events.lock().unwrap().push(payload);
    }
}

/// Convenience: build an `Arc<dyn EventSink>` from an `AppHandle`.
pub fn from_app_handle(app: tauri::AppHandle) -> Arc<dyn EventSink> {
    Arc::new(TauriEventSink::new(app))
}
