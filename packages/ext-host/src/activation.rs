//! Lazy activation.
//!
//! Activation events (`language:<id>`, `command:<glob>`, `view:<id>`,
//! `source:<family>`, `startup`) are matched against things happening in the
//! shell; the first match instantiates the runtime for that extension and
//! calls `activate`. Activation is async and must never block the UI thread;
//! commands issued to a not-yet-active extension are queued.
