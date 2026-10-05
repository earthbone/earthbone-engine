// SPDX-License-Identifier: GPL-3.0-or-later

//! Release timing probe using PRF-12's shared synthetic record builder.

#[allow(unused_imports)] // CONTEXT: The shared fixture also imports test-only frame helpers.
#[path = "../tests/prf_12_frame.rs"]
mod prf_12_frame;

fn main() -> Result<(), String> {
    soe_trace::Profile::disable();
    soe_trace::Trace::disable();
    if std::env::args().any(|argument| argument == "suspended") {
        soe_trace::with_timing_suspended(probe)
    } else {
        probe()
    }
}

fn probe() -> Result<(), String> {
    let (mut game, actors) = prf_12_frame::actors(1000)?;
    for _ in 0..10 {
        game.tick(1.0 / 60.0);
    }
    let started = std::time::Instant::now();
    for _ in 0..240 {
        game.tick(1.0 / 60.0);
    }
    let elapsed = started.elapsed().as_nanos();
    let world = game.world();
    let mut checksum = 0u64;
    for actor in actors {
        let reference = world.reference_ref(actor).ok_or("missing stress actor")?;
        for component in reference.position {
            checksum = checksum.wrapping_mul(16777619) ^ u64::from(component.to_bits());
        }
    }
    println!("{elapsed} {checksum}");
    Ok(())
}
