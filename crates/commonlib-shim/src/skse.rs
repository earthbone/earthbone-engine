// SPDX-License-Identifier: MIT

//! The `SKSE::` plugin interfaces as traits. Every interface the four
//! sample plugins query is here except the trampoline, which patches
//! machine code and has no meaning against this engine.

/// `SKSE::PluginHandle`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PluginHandle(pub u32);

impl PluginHandle {
    pub const INVALID: Self = Self(u32::MAX);
}

/// `SKSE::PluginVersionData`, the fields a plugin declares about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDeclaration {
    pub name: String,
    pub version: u32,
    pub author: String,
}

/// `SKSE::MessagingInterface::Message` kinds the sample plugins listen for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MessageKind {
    PostLoad,
    PostPostLoad,
    DataLoaded,
    NewGame,
    PreLoadGame,
    PostLoadGame,
    SaveGame,
    DeleteGame,
}

pub trait MessagingInterface {
    fn register_listener(&mut self, plugin: PluginHandle, listener: Box<dyn FnMut(MessageKind)>);
    fn dispatch(&mut self, kind: MessageKind);
}

/// `SKSE::SerializationInterface`: the cosave. Records are keyed by a
/// four-byte type and a version, exactly as the `.skse` file stores them.
pub trait SerializationInterface {
    fn set_unique_id(&mut self, plugin: PluginHandle, id: [u8; 4]);
    fn write_record(&mut self, kind: [u8; 4], version: u32, data: &[u8]) -> bool;
    fn read_record(&mut self) -> Option<([u8; 4], u32, Vec<u8>)>;
    /// `ResolveFormID`: map a saved form id to the current load order.
    fn resolve_form_id(&self, saved: crate::form::FormId) -> Option<crate::form::FormId>;
}

/// `SKSE::TaskInterface`: run on the game thread or the UI thread.
pub trait TaskInterface {
    fn add_task(&mut self, task: Box<dyn FnOnce() + Send>);
    fn add_ui_task(&mut self, task: Box<dyn FnOnce() + Send>);
}

/// `SKSE::PapyrusInterface::Register`: the registration callback receives
/// the native registry.
pub trait PapyrusInterface {
    fn register(&mut self, f: Box<dyn FnOnce(&mut crate::vm::NativeRegistry) -> bool>) -> bool;
}

/// What a plugin gets back for `SKSE::GetTrampoline` and every
/// `write_call`, `write_branch` and `write_vfunc`: nothing. The engine
/// exposes events and settings for the same purposes, listed per hook
/// site in `docs/shim.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported {
    pub what: &'static str,
}

pub const TRAMPOLINE: Unsupported = Unsupported {
    what: "SKSE::Trampoline patches machine code and has no target in this engine",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_invalid_handle_matches_commonlib() {
        assert_eq!(PluginHandle::INVALID.0, 0xFFFF_FFFF);
        assert!(TRAMPOLINE.what.contains("no target"));
    }
}
