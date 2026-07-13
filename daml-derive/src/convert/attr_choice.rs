use syn::{FnArg, ImplItem, ImplItemFn, Pat, PatType, ReturnType, Type};

use crate::convert::{AttrField, AttrType, data_type_string_from_path};

#[derive(Debug)]
pub struct AttrChoice {
    pub name: String,
    pub arguments: Vec<AttrField>,
    pub return_type: AttrType,
}

pub fn extract_all_choices(items: &[ImplItem]) -> Vec<AttrChoice> {
    items.iter().filter_map(get_single_attr_method).collect()
}

fn get_single_attr_method(impl_item: &ImplItem) -> Option<AttrChoice> {
    if let ImplItem::Fn(ImplItemFn {
        attrs,
        sig,
        ..
    }) = impl_item
    {
        match attrs.as_slice() {
            [attr] => Some(AttrChoice {
                name: data_type_string_from_path(attr.path()),
                arguments: sig.inputs.iter().filter_map(self::simple_method_name_and_type).collect(),
                return_type: output_type(&sig.output),
            }),
            _ => None,
        }
    } else {
        None
    }
}

fn output_type(return_type: &ReturnType) -> AttrType {
    match return_type {
        ReturnType::Default => AttrType::Unit,
        ReturnType::r#Type(_, ty) => AttrType::from_type(ty),
    }
}

fn simple_method_name_and_type(arg: &FnArg) -> Option<AttrField> {
    match arg {
        FnArg::Typed(PatType {
            pat,
            ty,
            ..
        }) => match pat.as_ref() {
            Pat::Ident(pat_ident) => match ty.as_ref() {
                Type::Path(type_path) => Some(AttrField {
                    field_label: pat_ident.ident.clone().to_string(),
                    field_type: AttrType::from_path(&type_path.path),
                }),
                _ => None,
            },
            _ => None,
        },
        FnArg::Receiver(_) => None,
    }
}
