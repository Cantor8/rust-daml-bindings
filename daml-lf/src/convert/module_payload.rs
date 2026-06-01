use crate::convert::data_payload::DamlDataPayload;
use crate::convert::interned::PackageInternedResolver;
use crate::lf_protobuf::daml_lf_2;

/// Borrowed view of an LF2 `Module` plus the indices needed to
/// resolve its name.
///
/// 3.2 keeps the wrapper small — just the name index and feature
/// flags. The fields that hold data types, templates, interfaces,
/// exceptions, and values land in 3.3+ as their conversions arrive.
#[derive(Debug)]
pub struct DamlModulePayload<'a> {
    pub name_index: i32,
    pub flags: ModuleFlags,
    pub module: &'a daml_lf_2::Module,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ModuleFlags {
    pub forbid_party_literals: bool,
    pub dont_divulge_contract_ids_in_create_arguments: bool,
    pub dont_disclose_non_consuming_choices_to_observers: bool,
}

impl<'a> DamlModulePayload<'a> {
    pub fn new(module: &'a daml_lf_2::Module) -> Self {
        let flags = module
            .flags
            .as_ref()
            .map(|f| ModuleFlags {
                forbid_party_literals: f.forbid_party_literals,
                dont_divulge_contract_ids_in_create_arguments: f.dont_divulge_contract_ids_in_create_arguments,
                dont_disclose_non_consuming_choices_to_observers: f.dont_disclose_non_consuming_choices_to_observers,
            })
            .unwrap_or_default();
        Self {
            name_index: module.name_interned_dname,
            flags,
            module,
        }
    }

    /// Resolve this module's dotted name (e.g. `["Foo", "Bar"]`)
    /// against the package-level interning tables.
    pub fn path<'b, R: PackageInternedResolver>(
        &self,
        resolver: &'b R,
    ) -> crate::error::DamlLfConvertResult<Vec<&'b str>> {
        resolver.resolve_dotted(self.name_index)
    }

    /// Iterate over the data-type definitions in this module
    /// (records, variants, enums, plus the LF2 marker entries that
    /// correspond to interface view-types).
    pub fn data_types(&self) -> impl Iterator<Item = DamlDataPayload<'a>> + '_ {
        self.module.data_types.iter().map(DamlDataPayload::new)
    }
}
