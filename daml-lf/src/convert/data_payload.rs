use crate::lf_protobuf::daml_lf_2;
use crate::lf_protobuf::daml_lf_2::def_data_type::DataCons;

/// Borrowed view of an LF2 `DefDataType` plus the precomputed name
/// index. The actual conversion to the element-layer [`DamlData`]
/// happens in `convert.rs`; this wrapper just gives callers a typed
/// handle over the LF2 message.
#[derive(Debug, Clone, Copy)]
pub struct DamlDataPayload<'a> {
    pub def: &'a daml_lf_2::DefDataType,
}

impl<'a> DamlDataPayload<'a> {
    pub const fn new(def: &'a daml_lf_2::DefDataType) -> Self {
        Self {
            def,
        }
    }

    pub const fn name_index(&self) -> i32 {
        self.def.name_interned_dname
    }

    pub const fn serializable(&self) -> bool {
        self.def.serializable
    }

    pub fn data_cons(&self) -> Option<&'a DataCons> {
        self.def.data_cons.as_ref()
    }

    pub fn params(&self) -> &'a [daml_lf_2::TypeVarWithKind] {
        &self.def.params
    }
}
