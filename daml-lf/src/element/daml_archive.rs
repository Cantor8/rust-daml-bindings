use crate::element::daml_package::DamlPackage;
use crate::element::visitor::{DamlElementVisitor, DamlVisitableElement};
use crate::element::{
    DamlChoice, DamlData, DamlInterface, DamlModule, DamlTemplate, DamlTyCon, DamlTyConName, DamlType, serialize,
};
#[cfg(feature = "full")]
use crate::element::{DamlDefValue, DamlValueName};
use crate::error::{DamlLfConvertError, DamlLfConvertResult};
use bounded_static::ToStatic;
use itertools::Itertools;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;

/// A Daml Archive.
#[derive(Debug, Serialize, Clone, Default, ToStatic)]
pub struct DamlArchive<'a> {
    name: Cow<'a, str>,
    main_package_id: Cow<'a, str>,
    #[serde(serialize_with = "serialize::serialize_map")]
    packages: HashMap<Cow<'a, str>, DamlPackage<'a>>,
}

impl<'a> DamlArchive<'a> {
    ///
    pub const fn new(
        name: Cow<'a, str>,
        main_package_id: Cow<'a, str>,
        packages: HashMap<Cow<'a, str>, DamlPackage<'a>>,
    ) -> Self {
        Self {
            name,
            main_package_id,
            packages,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the package id of the main `DamlPackage` contained in this `DamlArchive`.
    pub fn main_package_id(&self) -> &str {
        &self.main_package_id
    }

    /// Return an Iterator of the [`DamlPackage`] in this [`DamlArchive`].
    pub fn packages(&self) -> impl Iterator<Item = &DamlPackage<'_>> {
        self.packages.values()
    }

    /// Return the first [`DamlPackage`] in this [`DamlArchive`] which has the given `name` or `None` if no such
    /// package exists.
    pub fn package_by_name(&self, name: &str) -> Option<&DamlPackage<'_>> {
        self.packages.values().find(|p| p.name() == name)
    }

    /// Return the main [`DamlPackage`] in this [`DamlArchive`] or `None` if no such package exists.
    pub fn main_package(&self) -> Option<&DamlPackage<'_>> {
        self.packages.get(&self.main_package_id)
    }

    /// Retrieve a `DamlData` contained within this `DamlArchive` referred to by the supplied `DamlTyCon` or `None` if
    /// not such data item exists.
    ///
    /// DOCME
    pub fn data_by_tycon<'b>(&'a self, tycon: &'b DamlTyCon<'_>) -> Option<&'a DamlData<'a>> {
        self.data_by_tycon_name(tycon.tycon())
    }

    /// Retrieve a `DamlData` contained within this `DamlArchive` referred to by the supplied `DamlTyConName` or `None`
    /// if not such data item exists.
    ///
    /// DOCME
    pub fn data_by_tycon_name<'b>(&'a self, tycon_name: &'b DamlTyConName<'_>) -> Option<&'a DamlData<'a>> {
        let (package_id, module_path, data_name) = tycon_name.reference_parts();
        self.data(package_id, module_path, data_name)
    }

    /// Retrieve a `DamlData` contained within this `DamlArchive` referred to by the supplied package id, module path &
    /// name or `None` if not such data item exists.
    ///
    /// DOCME
    pub fn data<P, M, D>(&'a self, package_id: P, module_path: &[M], data_name: D) -> Option<&'a DamlData<'a>>
    where
        P: AsRef<str>,
        M: AsRef<str>,
        D: AsRef<str>,
    {
        self.packages
            .get(package_id.as_ref())?
            .root_module()
            .child_module_path(module_path)?
            .data_type(data_name.as_ref())
    }

    /// Retrieve a [`crate::element::DamlInterface`] by its tycon name, or `None` if
    /// no such interface exists in the archive.
    pub fn interface_by_tycon_name<'b>(
        &'a self,
        tycon_name: &'b crate::element::DamlTyConName<'_>,
    ) -> Option<&'a crate::element::DamlInterface<'a>> {
        let (package_id, module_path, name) = tycon_name.reference_parts();
        self.interface(package_id, module_path, name)
    }

    /// Retrieve a [`crate::element::DamlInterface`] by package-id, module path, and
    /// interface name, or `None` if no such interface exists.
    pub fn interface<P, M, D>(
        &'a self,
        package_id: P,
        module_path: &[M],
        name: D,
    ) -> Option<&'a crate::element::DamlInterface<'a>>
    where
        P: AsRef<str>,
        M: AsRef<str>,
        D: AsRef<str>,
    {
        self.packages
            .get(package_id.as_ref())?
            .root_module()
            .child_module_path(module_path)?
            .interfaces()
            .find(|i| i.name() == name.as_ref())
    }

    /// Retrieve a `DamlDefValue` for a given `DamlValueName` or `None` if no such value exists in this `DamlArchive`.
    ///
    /// DOCME
    #[cfg(feature = "full")]
    pub fn value_by_name<'b>(&'a self, name: &'b DamlValueName<'_>) -> Option<&'a DamlDefValue<'a>> {
        let (package_id, module_path, name) = name.reference_parts();
        self.value(package_id, module_path, name)
    }

    /// Retrieve a `DamlDefValue` for the supplied package id, module path & name or `None` if no such value exists in
    /// this `DamlArchive`.
    ///
    /// DOCME
    #[cfg(feature = "full")]
    pub fn value<P, M, D>(&'a self, package_id: P, module_path: &[M], name: D) -> Option<&'a DamlDefValue<'a>>
    where
        P: AsRef<str>,
        M: AsRef<str>,
        D: AsRef<str>,
    {
        self.packages.get(package_id.as_ref())?.root_module().child_module_path(module_path)?.value(name.as_ref())
    }

    /// Validate cross-references and shape constraints across the
    /// assembled archive. Walks every TyCon reference, template
    /// choice and interface and returns the first violation as a
    /// [`DamlLfConvertError`]:
    ///
    /// - [`UnknownPackage`] / [`UnknownModule`] / [`UnknownData`]:
    ///   a TyCon reference points at a target that doesn't exist.
    /// - [`UnexpectedChoiceData`]: a template choice's argument
    ///   type doesn't resolve to a Record.
    /// - [`UnexpectedType`]: an interface's view type doesn't
    ///   resolve to a Record.
    ///
    /// Returns `Ok(())` if every reference resolves and every shape
    /// holds. The walk is cheap to run after [`DarFile::apply`] /
    /// [`DarFile::to_owned_archive`] and is the recommended way to
    /// catch DAR-integrity problems eagerly instead of letting
    /// downstream `data_by_tycon_name` calls silently return `None`.
    ///
    /// [`UnknownPackage`]: DamlLfConvertError::UnknownPackage
    /// [`UnknownModule`]: DamlLfConvertError::UnknownModule
    /// [`UnknownData`]: DamlLfConvertError::UnknownData
    /// [`UnexpectedChoiceData`]: DamlLfConvertError::UnexpectedChoiceData
    /// [`UnexpectedType`]: DamlLfConvertError::UnexpectedType
    /// [`DarFile::apply`]: crate::DarFile::apply
    /// [`DarFile::to_owned_archive`]: crate::DarFile::to_owned_archive
    pub fn validate(&'a self) -> DamlLfConvertResult<()> {
        let mut modules: Vec<&DamlModule<'_>> = self.packages.values().map(DamlPackage::root_module).collect();
        while let Some(module) = modules.pop() {
            for data in module.data_types() {
                self.validate_data(data)?;
            }
            for interface in module.interfaces() {
                self.validate_interface(interface)?;
            }
            modules.extend(module.child_modules());
        }
        Ok(())
    }

    fn validate_data(&'a self, data: &DamlData<'a>) -> DamlLfConvertResult<()> {
        match data {
            DamlData::Record(rec) => {
                for field in rec.fields() {
                    self.validate_type(field.ty())?;
                }
            },
            DamlData::Variant(var) => {
                for field in var.fields() {
                    self.validate_type(field.ty())?;
                }
            },
            DamlData::Enum(_) => {},
            DamlData::Template(tpl) => self.validate_template(tpl)?,
        }
        Ok(())
    }

    fn validate_template(&'a self, template: &DamlTemplate<'a>) -> DamlLfConvertResult<()> {
        for field in template.fields() {
            self.validate_type(field.ty())?;
        }
        for choice in template.choices() {
            self.validate_choice(choice)?;
        }
        if let Some(key) = template.key() {
            self.validate_type(key.ty())?;
        }
        Ok(())
    }

    fn validate_interface(&'a self, interface: &DamlInterface<'a>) -> DamlLfConvertResult<()> {
        let view = interface.view();
        if !self.resolves_to_record(view) {
            return Err(DamlLfConvertError::UnexpectedType("Record".into(), format!("{view:?}")));
        }
        self.validate_type(view)?;
        for method in interface.methods() {
            self.validate_type(method.ty())?;
        }
        for choice in interface.choices() {
            self.validate_choice(choice)?;
        }
        for required in interface.requires() {
            self.validate_tycon_name(required)?;
        }
        Ok(())
    }

    fn validate_choice(&'a self, choice: &DamlChoice<'a>) -> DamlLfConvertResult<()> {
        let arg_ty = choice.fields().first().map(crate::element::DamlField::ty);
        match arg_ty {
            Some(ty) if self.resolves_to_record(ty) => self.validate_type(ty)?,
            _ => return Err(DamlLfConvertError::UnexpectedChoiceData),
        }
        self.validate_type(choice.return_type())?;
        Ok(())
    }

    /// Iterative walk over a type tree. Deeply-nested LF types
    /// (interned-type chains, big generic instantiations) easily
    /// exhaust the default 2MiB test-thread stack; a `Vec` work-list
    /// keeps the walk allocation-bounded by tree width, not depth.
    fn validate_type<'b>(&'a self, ty: &'b DamlType<'b>) -> DamlLfConvertResult<()> {
        let mut stack: Vec<&DamlType<'_>> = vec![ty];
        while let Some(t) = stack.pop() {
            match t {
                DamlType::TyCon(tycon) | DamlType::BoxedTyCon(tycon) => {
                    self.validate_tycon_name(tycon.tycon())?;
                    stack.extend(tycon.type_arguments().iter());
                },
                DamlType::ContractId(inner) => {
                    if let Some(boxed) = inner {
                        stack.push(boxed);
                    }
                },
                DamlType::Numeric(args)
                | DamlType::List(args)
                | DamlType::TextMap(args)
                | DamlType::GenMap(args)
                | DamlType::Optional(args) => stack.extend(args.iter()),
                DamlType::Var(var) => stack.extend(var.type_arguments().iter()),
                DamlType::Forall(forall) => stack.push(forall.body()),
                DamlType::Struct(s) => stack.extend(s.fields().iter().map(crate::element::DamlField::ty)),
                DamlType::Syn(syn) => stack.extend(syn.args().iter()),
                DamlType::Text
                | DamlType::Int64
                | DamlType::Timestamp
                | DamlType::Party
                | DamlType::Bool
                | DamlType::Unit
                | DamlType::Date
                | DamlType::Nat(_)
                | DamlType::Arrow
                | DamlType::Any
                | DamlType::TypeRep
                | DamlType::Bignumeric
                | DamlType::RoundingMode
                | DamlType::AnyException
                | DamlType::Update
                | DamlType::FailureCategory => {},
            }
        }
        Ok(())
    }

    fn validate_tycon_name(&'a self, name: &DamlTyConName<'_>) -> DamlLfConvertResult<()> {
        if self.data_by_tycon_name(name).is_some() || self.interface_by_tycon_name(name).is_some() {
            return Ok(());
        }
        let (pkg_id, module_path, data_name) = name.reference_parts();
        if !self.packages.contains_key(pkg_id) {
            return Err(DamlLfConvertError::UnknownPackage(pkg_id.to_string()));
        }
        let module_segments: Vec<&str> = module_path.iter().map(Cow::as_ref).collect();
        if self.packages.get(pkg_id).and_then(|p| p.root_module().child_module_path(&module_segments)).is_none() {
            return Err(DamlLfConvertError::UnknownModule(module_segments.join(".")));
        }
        Err(DamlLfConvertError::UnknownData(data_name.to_string()))
    }

    fn resolves_to_record(&'a self, ty: &DamlType<'_>) -> bool {
        match ty {
            DamlType::TyCon(tycon) | DamlType::BoxedTyCon(tycon) => {
                matches!(self.data_by_tycon_name(tycon.tycon()), Some(DamlData::Record(_)))
            },
            _ => false,
        }
    }
}

impl<'a> DamlVisitableElement<'a> for DamlArchive<'a> {
    fn accept(&'a self, visitor: &'a mut impl DamlElementVisitor) {
        visitor.pre_visit_archive(self);
        if visitor.sort_elements() {
            self.packages.values().sorted_by_key(|p| p.name()).for_each(|package| package.accept(visitor));
        } else {
            self.packages.values().for_each(|package| package.accept(visitor));
        }
        visitor.post_visit_archive(self);
    }
}
