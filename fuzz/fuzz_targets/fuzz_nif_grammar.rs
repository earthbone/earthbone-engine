#![no_main]

//! Grammar driven NIF fuzzing. Each generated input has a stream-100 header,
//! complete block and string tables, a valid scene root, and a packed triangle.
//! The fuzzer therefore reaches the NiNode, BSTriShape, vertex and mesh readers
//! while mutating bounded geometry fields.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|plan: soe_fuzz::nif_grammar::Plan| {
    let bytes = soe_fuzz::nif_grammar::build(&plan);
    let Ok(file) = soe_nif::parse(&bytes) else {
        panic!("the NIF grammar emitted a file rejected by the parser");
    };
    let meshes = file
        .meshes()
        .expect("generated triangle should form a mesh");
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].triangle_count(), 1);
});
