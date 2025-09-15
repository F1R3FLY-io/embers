use proc_macro::TokenStream;

mod into_rho_value;
mod render;

#[proc_macro_derive(IntoRhoValue)]
pub fn into_rho_value_derive(input: TokenStream) -> TokenStream {
    into_rho_value::into_rho_value_derive(input)
}

#[proc_macro_derive(Render, attributes(template))]
pub fn render_derive(input: TokenStream) -> TokenStream {
    render::render_derive(input)
}
