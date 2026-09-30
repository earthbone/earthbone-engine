// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Papyrus for the Skyrim open engine: a PEX loader, a small assembler that
//! emits PEX, and a virtual machine with a scheduler, latent calls and a native
//! function registry.
//!
//! The PEX layout follows the UESP Compiled Script File Format as decoded by
//! Champollion (`engine-refs/Champollion/Pex`) and skymp's reader. Skyrim files
//! (format 3.1 and 3.2) are big-endian, Fallout 4 files little-endian, and the
//! magic number tells the two apart. Only the Skyrim object layout is accepted.
//!
//! The VM keeps every script function call as a frame on a thread's own stack,
//! so a latent native or `Utility.Wait` can suspend a thread at any call depth
//! and resume it later. That is the part skymp's papyrus-vm lacks.

pub mod asm;
pub mod pex;
pub mod psc;
pub mod value;
pub mod vm;

pub use pex::{Function, Instruction, Object, Opcode, Property, Script, State, Variable};
pub use value::{ArrayRef, ObjectId, StructRef, Value};
pub use vm::{
    Log, NativeCall, NativeFn, NativeRegistry, NativeReturn, ThreadId, ThreadState, Vm, VmError,
    MAX_ARRAY_LEN, MAX_STRING_LEN,
};
