use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

pub(crate) fn into_rho_value_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let expanded = match input.data {
        Data::Struct(data) => impl_for_struct(name, data.fields),
        Data::Enum(_) => panic!("`IntoRhoValue` cannot be derived for enum"),
        Data::Union(_) => panic!("`IntoRhoValue` cannot be derived for unions"),
    };

    TokenStream::from(expanded)
}

fn impl_for_struct(name: syn::Ident, fields: Fields) -> proc_macro2::TokenStream {
    let into_rho_value_impl = match fields {
        Fields::Named(fields) => {
            let field_initializers = fields.named.into_iter().map(|f| {
                let field_name = f.ident.unwrap();
                let field_name_str = field_name.to_string();
                quote! {
                    map.insert(
                        ::std::borrow::ToOwned::to_owned(#field_name_str),
                        ::firefly_client::rendering::IntoRhoValue::into_rho_value(self.#field_name),
                    );
                }
            });
            quote! {
                let mut map = ::std::collections::BTreeMap::new();
                #(#field_initializers)*
                ::firefly_client::rendering::Value::Map(map)
            }
        }
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            quote! {
                ::firefly_client::rendering::IntoRhoValue::into_rho_value(self.0)
            }
        }
        Fields::Unnamed(fields) => {
            let field_initializers = fields.unnamed.into_iter().enumerate().map(|(i, _)| {
                quote! {
                    ::firefly_client::rendering::IntoRhoValue::into_rho_value(self.#i)
                }
            });
            quote! {
                ::firefly_client::rendering::Value::Tuple(::std::vec![#(#field_initializers),*])
            }
        }
        Fields::Unit => panic!("`IntoRhoValue` cannot be derived for unit struct"),
    };

    quote! {
        impl ::firefly_client::rendering::IntoRhoValue for #name {
            fn into_rho_value(self) -> ::firefly_client::rendering::Value {
                #into_rho_value_impl
            }
        }
    }
}
