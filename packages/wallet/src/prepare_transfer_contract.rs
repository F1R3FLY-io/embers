use etc::WalletAddress;

const TEMPLATE: &str = r#"
//FIREFLY_OPERATION;{"type": "TRANSFER", "wallet_address_from": "<%= wallet_address_from %>", "wallet_address_to": "<%= wallet_address_to %>", "amount": <%= amount %>, "description": "<%= description %>"}
new rl(`rho:registry:lookup`), RevVaultCh in {
    rl!(`rho:rchain:revVault`, *RevVaultCh) |
    for (@(_, RevVault) <- RevVaultCh) {
        new vaultCh, vaultTo, revVaultkeyCh,
        deployerId(`rho:rchain:deployerId`),
        deployId(`rho:rchain:deployId`)
        in {
            match ("<%= wallet_address_from %>", "<%= wallet_address_to %>", <%= amount %>) {
                (revAddrFrom, revAddrTo, amount) => {
                    @RevVault!("findOrCreate", revAddrFrom, *vaultCh) |
                    @RevVault!("findOrCreate", revAddrTo, *vaultTo) |
                    @RevVault!("deployerAuthKey", *deployerId, *revVaultkeyCh) |
                    for (@vault <- vaultCh; key <- revVaultkeyCh; _ <- vaultTo) {
                        match vault {
                            (true, vault) => {
                                new resultCh in {
                                    @vault!("transfer", revAddrTo, amount, *key, *resultCh) |
                                    for (@result <- resultCh) {
                                        match result {
                                            (true , _  ) => deployId!((true, "<%= description %>"))
                                            (false, err) => deployId!((false, err))
                                        }
                                    }
                                }
                            }
                            err => {
                                deployId!((false, "REV vault cannot be found or created"))
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;

pub(crate) fn create_transfer_contract(
    from: WalletAddress,
    to: WalletAddress,
    amount: u64,
    description: &str,
) -> String {
    TEMPLATE
        .replace("<%= wallet_address_from %>", from.as_str())
        .replace("<%= wallet_address_to %>", to.as_str())
        .replace("<%= amount %>", &amount.to_string())
        .replace("<%= description %>", description)
        .into()
}
