// SPDX-License-Identifier: MIT

//! The Papyrus VM surface: how a plugin registers natives and identifies
//! script objects. `soe-papyrus` owns the VM, this mirrors its registry rule.

use std::collections::BTreeMap;

/// `RE::StaticFunctionTag`: the receiver type of a global native.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct StaticFunctionTag;

/// `RE::VMHandle`, the id of a script object instance.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VmHandle(pub u64);

/// `RE::VMStackID`, the id of the running thread.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VmStackId(pub u32);

/// A native as the VM sees it: `Class.Function` and whether it is latent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeDecl {
    pub class: String,
    pub function: String,
    pub latent: bool,
}

/// The registry mirrors `soe-papyrus`: the first registration of a name
/// wins and later ones are reported, never applied. Two plugins that both
/// provide `PO3_SKSEFunctions.GetFormEditorID` behave the same as they do
/// today, where the first DLL loaded claims the name.
#[derive(Default, Debug)]
pub struct NativeRegistry {
    natives: BTreeMap<String, NativeDecl>,
    duplicates: Vec<NativeDecl>,
}

impl NativeRegistry {
    pub fn register(&mut self, decl: NativeDecl) -> bool {
        let key = format!("{}.{}", decl.class, decl.function).to_lowercase();
        if self.natives.contains_key(&key) {
            self.duplicates.push(decl);
            return false;
        }
        self.natives.insert(key, decl);
        true
    }

    pub fn get(&self, class: &str, function: &str) -> Option<&NativeDecl> {
        self.natives
            .get(&format!("{class}.{function}").to_lowercase())
    }

    pub fn duplicates(&self) -> &[NativeDecl] {
        &self.duplicates
    }

    pub fn len(&self) -> usize {
        self.natives.len()
    }

    pub fn is_empty(&self) -> bool {
        self.natives.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(class: &str, f: &str) -> NativeDecl {
        NativeDecl {
            class: class.into(),
            function: f.into(),
            latent: false,
        }
    }

    #[test]
    fn the_first_provider_of_a_name_wins() {
        let mut r = NativeRegistry::default();
        assert!(r.register(decl("PO3_SKSEFunctions", "GetFormEditorID")));
        assert!(!r.register(decl("po3_skseFunctions", "getformeditorid")));
        assert_eq!(r.len(), 1);
        assert_eq!(r.duplicates().len(), 1);
        assert!(r.get("PO3_SKSEFUNCTIONS", "GetFormEditorID").is_some());
    }
}
