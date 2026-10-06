// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{Span, Trace};
use std::sync::Mutex;

static TRACE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_23_tracy_keeps_chrome_events_and_dynamic_names() {
    let _guard = TRACE_TEST_LOCK.lock().expect("trace test lock");
    Trace::clear();
    Trace::enable();
    {
        let _static_span = Span::new("prf_23.static", "adversarial");
        let _dynamic_span = Span::dynamic(|| "Actor.DebugNative".to_owned(), "native");
    }

    let events = Trace::events();
    assert_eq!(events.len(), 2);
    assert!(events.iter().any(|event| event.name == "prf_23.static"));
    assert!(events.iter().any(|event| event.name == "Actor.DebugNative"));
    assert!(events.iter().all(|event| event.duration_ns > 0));
    Trace::disable();
    Trace::clear();
}

#[test]
fn prf_23_tracy_does_not_change_trace_disable_semantics() {
    let _guard = TRACE_TEST_LOCK.lock().expect("trace test lock");
    Trace::clear();
    Trace::disable();
    {
        let _static_span = Span::new("prf_23.disabled", "adversarial");
        let _dynamic_span = Span::dynamic(|| "must_not_be_recorded".to_owned(), "native");
    }
    assert!(Trace::events().is_empty());
    Trace::enable();
    Trace::clear();
}
