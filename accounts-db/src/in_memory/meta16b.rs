use modular_bitfield::prelude::*;

#[bitfield]
#[repr(u128)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta16B {
    /// Lamports and Rent Epoch mixed encoding (Scheme A)
    /// Bit 0: is_rent_exempt
    /// If 1: bits 1..64 = lamports
    /// If 0: bits 1..52 = lamports (max ~4.5e15), bits 52..64 = rent_epoch (max 4095)
    pub lamports_rent: B64,
    pub owner_idx: B16,
    pub data_offset: B32,
    pub data_pool_id: B4,
    pub flags: B12,
}

impl Meta16B {
    // Helper to get/set lamports based on Scheme A
    pub fn get_lamports(&self) -> u64 {
        let val = self.lamports_rent();
        if (val & 1) != 0 {
            val >> 1
        } else {
            (val >> 1) & 0x7FFFFFFFFFFFF // 51 bits
        }
    }

    pub fn get_rent_epoch(&self) -> u64 {
        let val = self.lamports_rent();
        if (val & 1) != 0 {
            u64::MAX // Exempt marker
        } else {
            val >> 52
        }
    }
}

// SPL Specific Flag Bit Indexes (inside the B12 flags field)
pub const SPL_FLAG_STATE_MASK: u16 = 0x3; // Bits 0-1
pub const SPL_FLAG_IS_NATIVE: u16 = 1 << 2;
pub const SPL_FLAG_HAS_DELEGATE: u16 = 1 << 3;
pub const SPL_FLAG_HAS_CLOSE_AUTH: u16 = 1 << 4;
pub const SPL_FLAG_HAS_DEL_AMT: u16 = 1 << 5;

#[repr(u8)]
pub enum SplAccountState {
    Uninitialized = 0,
    Initialized = 1,
    Frozen = 2,
}
