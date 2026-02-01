use crate::in_memory::meta16b::*;
use crate::in_memory::pubkey_registry::PubkeyRegistry;
use solana_sdk::pubkey::Pubkey;

pub struct SPLCompressor;

impl SPLCompressor {
    pub const TOKEN_PROGRAM_ID: Pubkey =
        solana_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

    /// Compress to the most compact tier possible
    /// Returns (Tier, bytes)
    pub fn compress(data: &[u8], registry: &PubkeyRegistry) -> (u8, Vec<u8>, u16) {
        if data.len() < 165 {
            return (11, data.to_vec(), 0); // Not a standard Token account?
        }

        // Field offsets for SPL Token Account
        // 0..32: Mint
        // 32..64: Owner
        // 64..72: Amount
        // 72..108: Delegate (COption<Pubkey>)
        // 108..109: State (u8)
        // 109..121: IsNative (COption<u64>)
        // 121..129: DelegatedAmount (u64)
        // 129..165: CloseAuthority (COption<Pubkey>)

        let mint = Pubkey::try_from(&data[0..32]).unwrap();
        let owner = Pubkey::try_from(&data[32..64]).unwrap();
        let amount = u64::from_le_bytes(data[64..72].try_into().unwrap());

        let delegate_tag = u32::from_le_bytes(data[72..76].try_into().unwrap());
        let state = data[108];
        let is_native_tag = u32::from_le_bytes(data[109..113].try_into().unwrap());
        let del_amt = u64::from_le_bytes(data[121..129].try_into().unwrap());
        let close_auth_tag = u32::from_le_bytes(data[129..133].try_into().unwrap());

        let mut flags: u16 = 0;
        // Set State bits
        flags |= (state as u16 & 0x3);

        let has_delegate = delegate_tag != 0;
        let has_del_amt = del_amt > 0;
        let has_close_auth = close_auth_tag != 0;
        let is_native = is_native_tag != 0;

        if is_native {
            flags |= SPL_FLAG_IS_NATIVE;
        }
        if has_delegate {
            flags |= SPL_FLAG_HAS_DELEGATE;
        }
        if has_close_auth {
            flags |= SPL_FLAG_HAS_CLOSE_AUTH;
        }
        if has_del_amt {
            flags |= SPL_FLAG_HAS_DEL_AMT;
        }

        let mint_id = registry.register(&mint);
        let owner_id = registry.register(&owner);

        // Selection logic from V10.4
        if !has_delegate && !has_close_auth && !has_del_amt && !is_native {
            // T1: 16B
            let mut buf = Vec::with_capacity(16);
            buf.extend_from_slice(&mint_id.to_le_bytes());
            buf.extend_from_slice(&owner_id.to_le_bytes());
            buf.extend_from_slice(&amount.to_le_bytes());
            (1, buf, flags)
        } else if !has_delegate && !has_close_auth && !has_del_amt && is_native {
            // T2: 32B
            let native_amt = u64::from_le_bytes(data[113..121].try_into().unwrap());
            let mut buf = Vec::with_capacity(32);
            buf.extend_from_slice(&mint_id.to_le_bytes());
            buf.extend_from_slice(&owner_id.to_le_bytes());
            buf.extend_from_slice(&amount.to_le_bytes());
            buf.extend_from_slice(&native_amt.to_le_bytes());
            buf.extend_from_slice(&[0u8; 8]); // Padding
            (2, buf, flags)
        } else {
            // T3: 48B
            let native_amt = if is_native {
                u64::from_le_bytes(data[113..121].try_into().unwrap())
            } else {
                0
            };
            let delegate_id = if has_delegate {
                let delegate_pk = Pubkey::try_from(&data[76..108]).unwrap();
                registry.register(&delegate_pk)
            } else {
                0
            };
            let close_auth_id = if has_close_auth {
                let close_auth_pk = Pubkey::try_from(&data[133..165]).unwrap();
                registry.register(&close_auth_pk)
            } else {
                0
            };

            let mut buf = Vec::with_capacity(48);
            buf.extend_from_slice(&mint_id.to_le_bytes());
            buf.extend_from_slice(&owner_id.to_le_bytes());
            buf.extend_from_slice(&amount.to_le_bytes());
            buf.extend_from_slice(&native_amt.to_le_bytes());
            buf.extend_from_slice(&del_amt.to_le_bytes());
            buf.extend_from_slice(&delegate_id.to_le_bytes());
            buf.extend_from_slice(&close_auth_id.to_le_bytes());
            buf.extend_from_slice(&[0u8; 8]); // Padding
            (3, buf, flags)
        }
    }

    pub fn decompress(
        tier: u8,
        compressed: &[u8],
        flags: u16,
        registry: &PubkeyRegistry,
    ) -> Vec<u8> {
        let mut data = vec![0u8; 165];

        let mint_id = u32::from_le_bytes(compressed[0..4].try_into().unwrap());
        let owner_id = u32::from_le_bytes(compressed[4..8].try_into().unwrap());
        let amount = u64::from_le_bytes(compressed[8..16].try_into().unwrap());

        let mint = registry.get_pubkey(mint_id).unwrap_or_default();
        let owner = registry.get_pubkey(owner_id).unwrap_or_default();

        data[0..32].copy_from_slice(mint.as_ref());
        data[32..64].copy_from_slice(owner.as_ref());
        data[64..72].copy_from_slice(&amount.to_le_bytes());

        // Decode Flags
        let state = (flags & SPL_FLAG_STATE_MASK) as u8;
        data[108] = state;

        if (flags & SPL_FLAG_IS_NATIVE) != 0 {
            data[109..113].copy_from_slice(&1u32.to_le_bytes()); // COption::Some
            let native_amt = if tier == 2 || tier == 3 {
                u64::from_le_bytes(compressed[16..24].try_into().unwrap())
            } else {
                0
            };
            data[113..121].copy_from_slice(&native_amt.to_le_bytes());
        }

        if tier == 3 {
            let del_amt = u64::from_le_bytes(compressed[24..32].try_into().unwrap());
            data[121..129].copy_from_slice(&del_amt.to_le_bytes());

            if (flags & SPL_FLAG_HAS_DELEGATE) != 0 {
                data[72..76].copy_from_slice(&1u32.to_le_bytes());
                let del_id = u32::from_le_bytes(compressed[32..36].try_into().unwrap());
                let del = registry.get_pubkey(del_id).unwrap_or_default();
                data[76..108].copy_from_slice(del.as_ref());
            }

            if (flags & SPL_FLAG_HAS_CLOSE_AUTH) != 0 {
                data[129..133].copy_from_slice(&1u32.to_le_bytes());
                let close_id = u32::from_le_bytes(compressed[36..40].try_into().unwrap());
                let close = registry.get_pubkey(close_id).unwrap_or_default();
                data[133..165].copy_from_slice(close.as_ref());
            }
        }

        data
    }
}
