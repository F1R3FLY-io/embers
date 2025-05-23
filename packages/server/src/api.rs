use poem_openapi::Tags;
pub mod wallet;

#[derive(Debug, Clone, Copy, Tags)]
enum Tag {
    Wallet,
}
