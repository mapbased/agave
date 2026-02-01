use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::RwLock;

pub struct PubkeyRegistry {
    inner: RwLock<RegistryInner>,
}

struct RegistryInner {
    forward: HashMap<Pubkey, u32>,
    reverse: Vec<Pubkey>,
}

impl PubkeyRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(RegistryInner {
                forward: HashMap::with_capacity(1_000_000),
                reverse: Vec::with_capacity(1_000_000),
            }),
        }
    }

    pub fn register(&self, pubkey: &Pubkey) -> u32 {
        {
            let read = self.inner.read().unwrap();
            if let Some(&id) = read.forward.get(pubkey) {
                return id;
            }
        }
        let mut write = self.inner.write().unwrap();
        // Double check after lock
        if let Some(&id) = write.forward.get(pubkey) {
            return id;
        }
        let id = write.reverse.len() as u32;
        write.forward.insert(*pubkey, id);
        write.reverse.push(*pubkey);
        id
    }

    pub fn get_id(&self, pubkey: &Pubkey) -> Option<u32> {
        self.inner.read().unwrap().forward.get(pubkey).copied()
    }

    pub fn get_pubkey(&self, id: u32) -> Option<Pubkey> {
        self.inner.read().unwrap().reverse.get(id as usize).copied()
    }
}
