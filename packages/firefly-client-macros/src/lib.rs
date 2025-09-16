use proc_macro::TokenStream;

mod into_value;
mod render;

#[proc_macro_derive(IntoValue)]
pub fn into_value_derive(input: TokenStream) -> TokenStream {
    into_value::into_value_derive(input)
}

#[proc_macro_derive(Render, attributes(template))]
pub fn render_derive(input: TokenStream) -> TokenStream {
    render::render_derive(input)
}
