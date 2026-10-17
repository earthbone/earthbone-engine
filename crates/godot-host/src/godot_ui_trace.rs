// SPDX-License-Identifier: GPL-3.0-or-later

//! GDScript UI scopes use the trace crate's shared clock and thread identity.
//! RefCounted lifetime follows Godot's local class_refcounted.rst contract.

use godot::classes::RefCounted;
use godot::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

static STOP: AtomicBool = AtomicBool::new(false);
static EXPORTER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

fn destination() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::var_os("SOE_UI_TRACE").map(PathBuf::from);
        if path.is_some() {
            soe_trace::Trace::enable();
            if let Some(path) = path.clone() {
                let worker = std::thread::Builder::new()
                    .name("ui-trace-export".into())
                    .spawn(move || {
                        while !STOP.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(500));
                            let temporary = path.with_extension("pending.json");
                            if soe_trace::Trace::export_chrome_json(&temporary)
                                .and_then(|()| std::fs::rename(&temporary, &path))
                                .is_err()
                            {
                                break;
                            }
                        }
                    });
                if let Ok(worker) = worker {
                    if let Ok(mut exporter) = EXPORTER.lock() {
                        *exporter = Some(worker);
                    }
                }
            }
        }
        path
    })
    .as_ref()
}

pub(crate) fn scope(name: impl FnOnce() -> String) -> soe_trace::Span {
    let _ = destination();
    soe_trace::Span::dynamic(name, "ui")
}

pub(crate) fn stop() {
    STOP.store(true, Ordering::Relaxed);
    if let Some(worker) = EXPORTER
        .lock()
        .ok()
        .and_then(|mut exporter| exporter.take())
    {
        let _ = worker.join();
    }
}

#[derive(GodotClass)]
#[class(init, base = RefCounted)]
struct UiTraceSpan {
    base: Base<RefCounted>,
    span: Option<soe_trace::Span>,
}

#[godot_api]
impl UiTraceSpan {
    #[func]
    fn start(&mut self, name: GString) {
        self.span = Some(self::scope(move || String::from(name)));
    }
}
