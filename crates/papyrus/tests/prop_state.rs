// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Generated PEX round trips and interrupted execution. The expected sum is
//! computed from the generator's inputs, independently of bytecode execution.

use proptest::prelude::*;
use soe_papyrus::asm::{self, f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, ByteOrder, Opcode};
use soe_papyrus::{Value, Vm};

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn generated_scripts_round_trip_and_resume_at_any_instruction(
        additions in prop::collection::vec(-1000i32..1000, 0..40),
        cut in 0usize..120,
        little in any::<bool>(),
        wait in any::<bool>(),
        objects in 1usize..6,
    ) {
        let mut function = FunctionBuilder::new("Run", "Int");
        for amount in &additions {
            function = function.ins(Opcode::IAdd, vec![ident("count"), ident("count"), i(*amount)]);
            if wait {
                function = function.ins(Opcode::CallStatic, vec![s("Utility"), s("Wait"), ident("::nonevar"), f(0.25)]);
            }
        }
        let script = ScriptBuilder::new("Generated", "")
            .byte_order(if little { ByteOrder::Little } else { ByteOrder::Big })
            .variable("count", "Int", i(0))
            .variable("first", "Int", i(17))
            .variable("second", "Int", i(-19))
            .function(function.ins(Opcode::Return, vec![ident("count")]).build())
            .build();
        let bytes = asm::write(&script);
        let parsed = pex::parse(&bytes).unwrap();
        prop_assert_eq!(&parsed, &script);
        prop_assert_eq!(asm::write(&parsed), bytes.clone());

        let mut uninterrupted = Vm::new();
        uninterrupted.load_bytes(&bytes).unwrap();
        let mut threads = Vec::new();
        for object in 0..objects {
            let object = uninterrupted.create_object(&format!("counter{object}"));
            uninterrupted.attach(object, "Generated").unwrap();
            threads.push(uninterrupted.call(object, "Run", &[]).unwrap());
        }
        uninterrupted.run(cut);
        let saved = uninterrupted.save_state().unwrap();
        let mut resumed = Vm::new();
        resumed.load_bytes(&bytes).unwrap();
        resumed.load_state(&saved).unwrap();
        prop_assert_eq!(resumed.save_state().unwrap(), saved);

        for _ in 0..=additions.len() {
            uninterrupted.advance(1.0);
            resumed.advance(1.0);
            uninterrupted.run(1000);
            resumed.run(1000);
            prop_assert_eq!(resumed.save_state().unwrap(), uninterrupted.save_state().unwrap());
        }
        let expected = Some(Value::Int(additions.iter().sum()));
        for thread in threads {
            prop_assert_eq!(resumed.thread_result(thread), expected.clone());
        }
    }
}
