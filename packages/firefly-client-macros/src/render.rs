use darling::{FromDeriveInput, FromField, FromVariant, ast};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitStr, parse_macro_input};

#[derive(Debug, Clone, FromDeriveInput)]
#[darling(
    attributes(template),
    supports(struct_named, struct_unit, enum_named, enum_unit)
)]
struct Args {
    ident: syn::Ident,
    generics: syn::Generics,
    path: Option<String>,
    blocks: Option<Vec<LitStr>>,
    data: ast::Data<EnumFieldArgs, StructFieldArgs>,
}

#[derive(Debug, Clone, FromField)]
#[darling(attributes(template), forward_attrs(allow, cfg))]
struct StructFieldArgs {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    attrs: Vec<syn::Attribute>,
    direct: Option<bool>,
}

#[derive(Debug, Clone, FromVariant)]
#[darling(attributes(template), forward_attrs(allow, cfg))]
struct EnumFieldArgs {
    ident: syn::Ident,
    attrs: Vec<syn::Attribute>,
    fields: ast::Fields<StructFieldArgs>,
    path: String,
}

pub fn render_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match Args::from_derive_input(&input) {
        Ok(v) => v,
        Err(err) => return err.write_errors().into(),
    };

    match args.data {
        ast::Data::Struct(fields) => TokenStream::from(impl_for_struct(
            &args.ident,
            &args.generics,
            &args.path.unwrap(),
            args.blocks.unwrap_or_default(),
            fields,
        )),
        ast::Data::Enum(items) => {
            TokenStream::from(impl_for_enum(&args.ident, &args.generics, &items))
        }
    }
}

fn impl_for_struct(
    ident: &syn::Ident,
    generics: &syn::Generics,
    path: &str,
    blocks: Vec<LitStr>,
    fields: ast::Fields<StructFieldArgs>,
) -> proc_macro2::TokenStream {
    let fields = fields.fields;

    let template_struct_fields = fields.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let attrs = &f.attrs;

        if f.direct == Some(true) {
            let type_ = &f.ty;
            quote! {
                #(#attrs)*
                #ident: #type_
            }
        } else {
            quote! {
                #(#attrs)*
                #ident: ::firefly_client::rendering::Value
            }
        }
    });

    let template_object_fields = fields.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let attrs = &f.attrs;

        if f.direct == Some(true) {
            quote! {
                #(#attrs)*
                #ident: self.#ident
            }
        } else {
            quote! {
                #(#attrs)*
                #ident: ::firefly_client::rendering::IntoValue::into_value(self.#ident)
            }
        }
    });

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics ::firefly_client::rendering::Render for #ident #ty_generics
            #where_clause
        {
            fn render(self) -> ::std::result::Result<::std::string::String, ::firefly_client::rendering::_dependencies::askama::Error> {
                #[derive(::firefly_client::rendering::_dependencies::askama::Template)]
                #[template(path = #path, blocks = [#(#blocks),*], escape = "none")]
                struct Template #ty_generics {
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

fn impl_for_enum(
    ident: &syn::Ident,
    generics: &syn::Generics,
    items: &[EnumFieldArgs],
) -> proc_macro2::TokenStream {
    let template_enum_items = items.iter().map(|item| {
        let ident = &item.ident;
        let attrs = &item.attrs;
        let path = &item.path;

        let fields = item.fields.fields.iter().map(|f| {
            let ident = &f.ident;
            let attrs = &f.attrs;

            if f.direct == Some(true) {
                let type_ = &f.ty;
                quote! {
                    #(#attrs)*
                    #ident: #type_
                }
            } else {
                quote! {
                    #(#attrs)*
                    #ident: ::firefly_client::rendering::Value
                }
            }
        });

        quote! {
            #(#attrs)*
            #[template(path = #path, escape = "none")]
            #ident {
                #(#fields),*
            }
        }
    });

    let template_enum_fields = items.iter().map(|item| {
        let ident = &item.ident;
        let attrs = &item.attrs;

        let variables = item.fields.fields.iter().map(|f| {
            let ident = &f.ident;
            let attrs = &f.attrs;

            quote! {
                #(#attrs)*
                #ident
            }
        });

        let mappings = item.fields.fields.iter().map(|f| {
            let ident = &f.ident;
            let attrs = &f.attrs;

            if f.direct == Some(true) {
                quote! {
                    #(#attrs)*
                    #ident: #ident
                }
            } else {
                quote! {
                    #(#attrs)*
                    #ident: ::firefly_client::rendering::IntoValue::into_value(#ident)
                }
            }
        });

        quote! {
            #(#attrs)*
            Self::#ident { #(#variables),* } => Template::#ident {
                #(#mappings),*
            }
        }
    });

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics ::firefly_client::rendering::Render for #ident #ty_generics
            #where_clause
        {
            fn render(self) -> ::std::result::Result<::std::string::String, ::firefly_client::rendering::_dependencies::askama::Error> {
                #[derive(::firefly_client::rendering::_dependencies::askama::Template)]
                enum Template #ty_generics {
                    #(#template_enum_items),*
                }

                let template = match self {
                    #(#template_enum_fields),*
                };

                ::firefly_client::rendering::_dependencies::askama::Template::render(&template)
            }
        }
    }
}
