use crate::convert::data_payload::DamlDataPayload;
use crate::convert::interned::PackageInternedResolver;
use crate::lf_protobuf::daml_lf_2;

/// Borrowed view of an LF2 `Module`: the precomputed name index +
/// feature flags plus a `&Module` reference whose data types,
/// templates, interfaces, exceptions, and values are read on demand
/// through the accessor methods below.
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

    /// Slice of the type-synonym definitions in this module.
    pub fn synonyms(&self) -> &'a [daml_lf_2::DefTypeSyn] {
        &self.module.synonyms
    }

    /// Slice of the template definitions in this module.
    pub fn templates(&self) -> &'a [daml_lf_2::DefTemplate] {
        &self.module.templates
    }

    /// Slice of the interface definitions in this module.
    pub fn interfaces(&self) -> &'a [daml_lf_2::DefInterface] {
        &self.module.interfaces
    }

    /// Slice of the exception definitions in this module.
    pub fn exceptions(&self) -> &'a [daml_lf_2::DefException] {
        &self.module.exceptions
    }

    /// Slice of the value definitions in this module. Consumed by
    /// `build_values` under `--features full`.
    #[allow(dead_code)]
    pub fn values(&self) -> &'a [daml_lf_2::DefValue] {
        &self.module.values
    }
}
