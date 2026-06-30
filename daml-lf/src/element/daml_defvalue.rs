use crate::element::daml_expr::DamlExpr;
use crate::element::{DamlElementVisitor, DamlType, DamlVisitableElement};
use bounded_static::ToStatic;
use serde::Serialize;
use std::borrow::Cow;

/// A Daml value (top-level `let`-bound definition).
///
/// LF2 dropped LF1's per-value `no_party_literals` and `is_test`
/// flags — `forbid_party_literals` is now a module-wide invariant
/// and "test" classification moved up to Daml Script. Neither field
/// is surfaced on this struct.
#[derive(Debug, Serialize, Clone, ToStatic)]
pub struct DamlDefValue<'a> {
    name: Cow<'a, str>,
    ty: DamlType<'a>,
    expr: DamlExpr<'a>,
}

impl<'a> DamlDefValue<'a> {
    pub const fn new(name: Cow<'a, str>, ty: DamlType<'a>, expr: DamlExpr<'a>) -> Self {
        Self {
            name,
            ty,
            expr,
        }
    }

    /// The name of this value.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name of this value.
    ///
    /// This is a clone of a `Cow<str>` which is cheap for the borrowed case used within the library.
    #[doc(hidden)]
    pub fn name_clone(&self) -> Cow<'a, str> {
        self.name.clone()
    }

    pub const fn ty(&self) -> &DamlType<'a> {
        &self.ty
    }

    pub const fn expr(&self) -> &DamlExpr<'a> {
        &self.expr
    }
}

impl<'a> DamlVisitableElement<'a> for DamlDefValue<'a> {
    fn accept(&'a self, visitor: &'a mut impl DamlElementVisitor) {
        visitor.pre_visit_def_value(self);
        self.ty.accept(visitor);
        self.expr.accept(visitor);
        visitor.post_visit_def_value(self);
    }
}
