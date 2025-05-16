use poem_openapi::Tags;

mod models;
pub mod wallet;

#[derive(Debug, Clone, Copy, Tags)]
enum Tag {
    Wallet,
}
