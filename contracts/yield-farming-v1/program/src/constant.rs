/// constants declaration file

/// mode of mainnet-beta or devnet, in case of mainnet-beta - const DEVNET_MODE:bool = false;

const DEVNET_MODE:bool = {
    #[cfg(feature = "devnet")]
    {
        true
    }
    #[cfg(not(feature = "devnet"))]
    {
        false
    }
};


/// Farm additaional fee
/// To create new farm without CRP token pairing, the creator must pay this additional farm fee as stable coin (USDC)
/// If the creator doesn't pay farm fee, displays "Not Allowed" instead of "Stake" button
/// So creator and farmers can't stake/unstake/harvest

pub const VERSION:u8 = 2;
pub const PREFIX:&str = "cropperfarm";

/// initial super owner of this program. this owner can change program state
pub const INITIAL_SUPER_OWNER:&str = if DEVNET_MODE {"4GJ3z4skEHJADz3MVeNYBg4YV8H27rBQey2YYdiPC8PA"} else {"AwtDEd9GThBNWNahvLZUok1BiRULNQ86VruXkYAckCtV"};

pub const TOKEN_PROGRAM_ID:&str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const RENT_SYSVAR_ID:&str = "SysvarRent111111111111111111111111111111111";
pub const CLOCK_SYSVAR_ID:&str = "SysvarC1ock11111111111111111111111111111111";
pub const SYSTEM_PROGRAM_ID:&str = "11111111111111111111111111111111";

/// Token mint addresses for specified farms above
pub const CRP_MINT_ADDRESS:&str = if DEVNET_MODE {"GGaUYeET8HXK34H2D1ieh4YYQPhkWcfWBZ4rdp6iCZtG"} else {"DubwWZNWiNGMMeeQHPnMATNj77YZPZSAz2WVR5WjLJqz"};
pub const USDC_MINT_ADDRESS:&str = if DEVNET_MODE {"6MBRfPbzejwVpADXq3LCotZetje3N16m5Yn7LCs2ffU4"} else {"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"};
pub const USDT_MINT_ADDRESS:&str = if DEVNET_MODE {"6La9ryWrDPByZViuQCizmo6aW98cK8DSL7angqmTFf9i"} else {"Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"};
pub const SOL_MINT_ADDRESS:&str = if DEVNET_MODE {"So11111111111111111111111111111111111111112"} else {"So11111111111111111111111111111111111111112"};
pub const ETH_MINT_ADDRESS:&str = if DEVNET_MODE {"2FPyTwcZLUg1MDrwsyoP4D6s1tM7hAkHYRjkNb5w6Pxk"} else {"2FPyTwcZLUg1MDrwsyoP4D6s1tM7hAkHYRjkNb5w6Pxk"};

/// reward multipler constant
pub const REWARD_MULTIPLER:u64 = 1000000000;

/// JUMP_SHARENET
pub const JUMP_DEBT:u64 = 10_000_000_000_000_000_000;


pub const REMOVE_REWARDS_FARM_ADDRESS:&str = if DEVNET_MODE {"Fv1ghuzaXvLmSFyMZoxbUBRJhDQp4ik4trak5c5rHuve"} else {"H9jkwKVS6YFCY87EuxF4P2z1yCJ4a4px1bpL1i49AGkB"};