use darling::{FromDeriveInput, FromField, ast};
use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[derive(Debug, Clone, FromDeriveInput)]
#[darling(supports(struct_any))]
struct Args {
    ident: syn::Ident,
    generics: syn::Generics,
    data: ast::Data<(), FieldArgs>,
}

#[derive(Debug, Clone, FromField)]
#[darling(forward_attrs(allow, cfg))]
struct FieldArgs {
    ident: Option<syn::Ident>,
    attrs: Vec<syn::Attribute>,
}

pub fn into_value_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match Args::from_derive_input(&input) {
        Ok(v) => v,
        Err(err) => return err.write_errors().into(),
    };

    TokenStream::from(impl_for_struct(args))
}

fn impl_for_struct(
    Args {
        ident,
        generics,
        data,
    }: Args,
) -> proc_macro2::TokenStream {
    let fields = data.take_struct().expect("expected a struct");
    let into_rho_value_impl = match fields.style {
        ast::Style::Tuple if fields.fields.len() == 1 => {
            quote! {
                ::firefly_client::rendering::IntoValue::into_value(self.0)
            }
        }
        ast::Style::Tuple => {
            let field_initializers = fields.fields.into_iter().enumerate().map(|(i, f)| {
                let attrs = &f.attrs;
                let lit = Literal::u64_unsuffixed(i as _);
                quote! {
                    #(#attrs)*
                    ::firefly_client::rendering::IntoValue::into_value(self.#lit)
                }
            });
            quote! {
                ::firefly_client::rendering::Value::Tuple(::std::vec![#(#field_initializers),*])
            }
        }
        ast::Style::Struct => {
            let field_initializers = fields.fields.into_iter().map(|f| {
                let field_name = f.ident.unwrap();
                let field_name_str = field_name.to_string();
                let attrs = &f.attrs;
                quote! {
                    #(#attrs)*
                    map.insert(
                        ::std::borrow::ToOwned::to_owned(#field_name_str),
                        ::firefly_client::rendering::IntoValue::into_value(self.#field_name),
                    );
                }
            });
            quote! {
                let mut map = ::std::collections::BTreeMap::new();
                #(#field_initializers)*
                ::firefly_client::rendering::Value::Map(map)
            }
        }
        ast::Style::Unit => quote! {
            ::firefly_client::rendering::Value::Tuple(::std::vec![])
        },
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics ::firefly_client::rendering::IntoValue for #ident #ty_generics
            #where_clause
        {
            fn into_value(self) -> ::firefly_client::rendering::Value {
                #into_rho_value_impl
            }
        }
    }
}
