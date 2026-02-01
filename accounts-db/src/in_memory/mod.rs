use crate::in_memory::arena::SubArena;
use crate::in_memory::ebr::{AsyncEbr, Guard};
use crate::in_memory::meta16b::*;
use crate::in_memory::pubkey_registry::PubkeyRegistry;
use crate::in_memory::spl_compressor::SPLCompressor;
use solana_sdk::pubkey::Pubkey;
use std::mem::transmute;
use std::sync::atomic::{AtomicU128, AtomicU32, Ordering};

pub struct AccountInfo {
    pub lamports: u64,
    pub owner: Pubkey,
    pub data: Vec<u8>,
    pub rent_epoch: u64,
    pub executable: bool,
}

pub struct InMemoryAccountsDb {
    pub meta_arena: SubArena,
    pub pools: Vec<SubArena>,
    pub pubkey_registry: PubkeyRegistry,
    pub owner_registry: Vec<AtomicU32>, // u16 index -> u32 AccountId
    pub ebr: AsyncEbr,
}

struct DeferPoolFree {
    pool_ptr: *const SubArena,
    offset: u32,
}

impl Drop for DeferPoolFree {
    fn drop(&mut self) {
        unsafe {
            let pool = &*self.pool_ptr;
            pool.free(self.offset);
        }
    }
}

impl InMemoryAccountsDb {
    pub const TIER_SIZES: [usize; 16] = [
        0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 256, 512, 1024, 8192,
    ];

    pub fn new() -> Self {
        let mut pools = Vec::with_capacity(16);
        for &size in Self::TIER_SIZES.iter() {
            let res_size = match size {
                0 => 0,
                16 | 32 => 64,
                _ => 16,
            };
            pools.push(SubArena::new(size.max(8), res_size));
        }

        let mut owner_registry = Vec::with_capacity(65536);
        for _ in 0..65536 {
            owner_registry.push(AtomicU32::new(0));
        }

        Self {
            meta_arena: SubArena::new(16, 32),
            pools,
            pubkey_registry: PubkeyRegistry::new(),
            owner_registry,
            ebr: AsyncEbr::new(),
        }
    }

    pub fn get_pool_id(len: usize) -> u8 {
        for (i, &size) in Self::TIER_SIZES.iter().enumerate() {
            if len <= size {
                return i as u8;
            }
        }
        15
    }

    pub unsafe fn load(&self, account_id: u32) -> Option<AccountInfo> {
        let guard = self.ebr.enter();
        let meta_ptr = self.meta_arena.get_ptr(account_id) as *const AtomicU128;
        if meta_ptr.is_null() {
            return None;
        }

        let meta_val = (*meta_ptr).load(Ordering::Acquire);
        if meta_val == 0 {
            return None;
        }
        let meta: Meta16B = transmute(meta_val);

        let lamports = meta.get_lamports();
        let rent_epoch = meta.get_rent_epoch();
        let owner_account_id =
            self.owner_registry[meta.owner_idx() as usize].load(Ordering::Relaxed);
        let owner = self.pubkey_registry.get_pubkey(owner_account_id)?;

        let pool_id = meta.data_pool_id() as usize;
        let data_offset = meta.data_offset();

        let data = if pool_id == 0 {
            vec![]
        } else {
            let pool = &self.pools[pool_id];
            let raw_data_ptr = pool.get_ptr(data_offset);
            let raw_data = std::slice::from_raw_parts(raw_data_ptr, Self::TIER_SIZES[pool_id]);

            if owner == SPLCompressor::TOKEN_PROGRAM_ID && pool_id <= 3 {
                SPLCompressor::decompress(
                    pool_id as u8,
                    raw_data,
                    meta.flags(),
                    &self.pubkey_registry,
                )
            } else {
                raw_data.to_vec()
            }
        };

        Some(AccountInfo {
            lamports,
            owner,
            data,
            rent_epoch,
            executable: (meta.flags() & 0x1) != 0, // is_executable in bit 0 of flags
        })
    }

    pub unsafe fn store(&self, account_id: u32, info: &AccountInfo) {
        let guard = self.ebr.enter();

        let (pool_id, data_buf, mut flags) = if info.owner == SPLCompressor::TOKEN_PROGRAM_ID {
            SPLCompressor::compress(&info.data, &self.pubkey_registry)
        } else {
            (Self::get_pool_id(info.data.len()), info.data.clone(), 0)
        };

        // Set is_executable in flags bit 0
        if info.executable {
            flags |= 1;
        }

        let mut data_offset = 0;
        if pool_id > 0 {
            let pool = &self.pools[pool_id as usize];
            data_offset = pool.alloc();
            let dest = pool.get_ptr(data_offset);
            std::ptr::copy_nonoverlapping(data_buf.as_ptr(), dest, data_buf.len());
        }

        // Prepare new Meta16B
        let mut new_meta = Meta16B::new();
        let is_exempt = info.lamports > 0; // Simplified
        let lamports_rent = if is_exempt {
            (info.lamports << 1) | 1
        } else {
            (info.lamports << 1) | (info.rent_epoch << 52)
        };

        let owner_idx = self.get_or_register_owner(info.owner);

        new_meta.set_lamports_rent(lamports_rent);
        new_meta.set_owner_idx(owner_idx);
        new_meta.set_data_offset(data_offset);
        new_meta.set_data_pool_id(pool_id as u8);
        new_meta.set_flags(flags);

        let meta_ptr = self.meta_arena.get_ptr(account_id) as *mut AtomicU128;
        let old_meta_val = (*meta_ptr).swap(transmute(new_meta), Ordering::AcqRel);

        // If existed, retire old data
        if old_meta_val != 0 {
            let old_meta: Meta16B = transmute(old_meta_val);
            self.retire_old_slot(&old_meta, &guard);
        }
    }

    fn retire_old_slot(&self, old_meta: &Meta16B, guard: &Guard) {
        let pool_id = old_meta.data_pool_id();
        let offset = old_meta.data_offset();
        if pool_id == 0 || offset == 0 {
            return;
        }

        let payload = Box::into_raw(Box::new(DeferPoolFree {
            pool_ptr: &self.pools[pool_id as usize],
            offset,
        }));

        guard.retire(payload);
    }

    fn get_or_register_owner(&self, owner: Pubkey) -> u16 {
        let account_id = self.pubkey_registry.register(&owner);
        // Practical scan or hashmap for owner_idx
        for (i, entry) in self.owner_registry.iter().enumerate() {
            let val = entry.load(Ordering::Relaxed);
            if val == account_id {
                return i as u16;
            }
            if val == 0 {
                if entry
                    .compare_exchange(0, account_id, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
                {
                    return i as u16;
                }
            }
        }
        0 // Overflow? Error handling needed
    }
}
