//! Shared valid inputs for byte-oriented fuzz targets and their seed checker.

pub fn state_probe() -> soe_papyrus::Vm {
    use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::pex::Opcode;
    let bytes = ScriptBuilder::new("StateProbe", "")
        .variable("count", "Int", i(0))
        .function(
            FunctionBuilder::new("Run", "Int")
                .ins(Opcode::IAdd, vec![ident("count"), ident("count"), i(1)])
                .ins(Opcode::Return, vec![ident("count")])
                .build(),
        )
        .bytes();
    let mut vm = soe_papyrus::Vm::new();
    vm.load_bytes(&bytes).expect("probe PEX");
    vm
}

pub fn state_seed() -> Vec<u8> {
    let mut vm = state_probe();
    let object = vm.create_object("probe");
    vm.attach(object, "StateProbe").expect("attach probe");
    vm.call(object, "Run", &[]).expect("start probe");
    vm.save_state().expect("save probe")
}
