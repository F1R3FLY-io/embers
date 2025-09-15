use darling::FromDeriveInput;
use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[derive(Debug, Clone, FromDeriveInput)]
#[darling(attributes(template))]
struct Args {
    ident: syn::Ident,
    path: String,
}

pub(crate) fn render_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match Args::from_derive_input(&input) {
        Ok(v) => v,
        Err(err) => return err.write_errors().into(),
    };

    let expanded = match input.data {
        Data::Struct(data) => impl_for_struct(args, data.fields),
        Data::Enum(_) => panic!("`IntoRhoValue` cannot be derived for enum"),
        Data::Union(_) => panic!("`IntoRhoValue` cannot be derived for unions"),
    };

    TokenStream::from(expanded)
}

fn impl_for_struct(args: Args, fields: Fields) -> proc_macro2::TokenStream {
    let Args { ident, path } = args;
    let fields: Vec<_> = match fields {
        Fields::Named(fields) => fields.named.into_iter().map(|f| f.ident.unwrap()).collect(),
        Fields::Unnamed(_) => panic!("`IntoRhoValue` cannot be derived for unnamed struct"),
        Fields::Unit => panic!("`IntoRhoValue` cannot be derived for unit struct"),
    };

    let template_struct_fields = fields.iter().map(|f| {
        quote! {
            #f: ::firefly_client::rendering::Value
        }
    });

    let template_object_fields = fields.iter().map(|f| {
        quote! {
            #f: ::firefly_client::rendering::IntoRhoValue::into_rho_value(self.#f)
        }
    });

    quote! {
        impl ::firefly_client::rendering::Render for #ident {
            fn render(self) -> ::std::result::Result<::std::string::String, ::firefly_client::rendering::_dependencies::askama::Error> {
                #[derive(::firefly_client::rendering::_dependencies::askama::Template)]
                #[template(path = #path, escape = "none")]
                struct Template {
                    #(#template_struct_fields),*
                }

                let template = Template {
                    #(#template_object_fields),*
                };

                ::firefly_client::rendering::_dependencies::askama::Template::render(&template)
            }
        }
    }
}
