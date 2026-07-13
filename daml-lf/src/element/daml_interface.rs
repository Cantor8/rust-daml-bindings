use std::borrow::Cow;

use bounded_static::ToStatic;
use serde::Serialize;

use crate::element::visitor::{DamlElementVisitor, DamlVisitableElement};
use crate::element::{DamlChoice, DamlTyConName, DamlType};

/// A Daml interface — LF2's first-class abstraction for templates
/// to implement.
///
/// Interfaces live alongside data types in the module tree but are
/// addressed by their own `DamlModule::interfaces` slot rather than
/// folded into `DamlData` (the data shapes are records/variants/
/// enums; interfaces are categorically different).
///
/// `view` carries the type of the interface's `view` method (the
/// "what does this contract look like through this interface" return
/// type). The view's expression body is `full`-gated.
#[derive(Debug, Serialize, Clone, ToStatic)]
pub struct DamlInterface<'a> {
    name: Cow<'a, str>,
    package_id: Cow<'a, str>,
    module_path: Vec<Cow<'a, str>>,
    /// Name to which the interface value ("this") is bound in
    /// preconditions and fixed choices.
    param: Cow<'a, str>,
    methods: Vec<DamlInterfaceMethod<'a>>,
    /// Fixed choices defined on the interface — implementations
    /// inherit these.
    choices: Vec<DamlChoice<'a>>,
    /// The view type returned by this interface's `view` method.
    view: DamlType<'a>,
    /// Other interfaces this interface requires; an implementing
    /// template must also implement everything in `requires`
    /// (transitively).
    requires: Vec<DamlTyConName<'a>>,
}

impl<'a> DamlInterface<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: Cow<'a, str>,
        package_id: Cow<'a, str>,
        module_path: Vec<Cow<'a, str>>,
        param: Cow<'a, str>,
        methods: Vec<DamlInterfaceMethod<'a>>,
        choices: Vec<DamlChoice<'a>>,
        view: DamlType<'a>,
        requires: Vec<DamlTyConName<'a>>,
    ) -> Self {
        Self {
            name,
            package_id,
            module_path,
            param,
            methods,
            choices,
            view,
            requires,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn module_path(&self) -> impl Iterator<Item = &str> {
        self.module_path.iter().map(AsRef::as_ref)
    }

    pub fn param(&self) -> &str {
        &self.param
    }

    pub fn methods(&self) -> &[DamlInterfaceMethod<'a>] {
        &self.methods
    }

    pub fn choices(&self) -> &[DamlChoice<'a>] {
        &self.choices
    }

    pub const fn view(&self) -> &DamlType<'a> {
        &self.view
    }

    pub fn requires(&self) -> &[DamlTyConName<'a>] {
        &self.requires
    }
}

impl<'a> DamlVisitableElement<'a> for DamlInterface<'a> {
    fn accept(&'a self, visitor: &'a mut impl DamlElementVisitor) {
        visitor.pre_visit_interface(self);
        self.methods.iter().for_each(|m| m.accept(visitor));
        self.choices.iter().for_each(|c| c.accept(visitor));
        self.view.accept(visitor);
        self.requires.iter().for_each(|r| r.accept(visitor));
        visitor.post_visit_interface(self);
    }
}

/// A method declaration on a [`DamlInterface`].
#[derive(Debug, Serialize, Clone, ToStatic)]
pub struct DamlInterfaceMethod<'a> {
    name: Cow<'a, str>,
    ty: DamlType<'a>,
}

impl<'a> DamlInterfaceMethod<'a> {
    pub const fn new(name: Cow<'a, str>, ty: DamlType<'a>) -> Self {
        Self {
            name,
            ty,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn ty(&self) -> &DamlType<'a> {
        &self.ty
    }
}

impl<'a> DamlVisitableElement<'a> for DamlInterfaceMethod<'a> {
    fn accept(&'a self, visitor: &'a mut impl DamlElementVisitor) {
        visitor.pre_visit_interface_method(self);
        self.ty.accept(visitor);
        visitor.post_visit_interface_method(self);
    }
}
