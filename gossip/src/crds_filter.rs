use {
    crate::{crds_data::CrdsData, crds_value::CrdsValue},
    solana_pubkey::Pubkey,
    std::{
        collections::HashMap,
        sync::atomic::{AtomicBool, Ordering},
    },
};

/// 极简模式开关：当开启时（如 runner/deshred 等非共识轻量节点），
/// 在验签前直接丢弃所有 Vote、EpochSlots、DuplicateShred 等共识消息，
/// 仅保留 ContactInfo，避免海量无用消息触发 Ed25519 验签占用大量 CPU。
pub static GOSSIP_MINIMAL_MODE: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn set_gossip_minimal_mode(enabled: bool) {
    GOSSIP_MINIMAL_MODE.store(enabled, Ordering::Relaxed);
}

#[inline]
pub fn is_gossip_minimal_mode() -> bool {
    GOSSIP_MINIMAL_MODE.load(Ordering::Relaxed)
}

/// Helper to check whether an incoming raw packet should be pre-dropped in minimal mode
/// before expensive deserialization, transaction parsing, and SHA256 computations.
///
/// In minimal mode (e.g. runner/deshred non-consensus nodes), we only need
/// PullResponse (tag 1) to discover peers and PongMessage (tag 5) / PingMessage (tag 4) for liveness.
/// Tag 0 (PullRequest), Tag 2 (PushMessage, 95%+ of gossip traffic), and Tag 3 (PruneMessage)
/// are dropped in 1 instruction.
#[inline]
pub fn should_pre_drop_gossip_packet(data: &[u8]) -> bool {
    if is_gossip_minimal_mode() {
        if data.len() < 4 {
            return true;
        }
        let tag = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        return tag == 0 || tag == 2 || tag == 3;
    }
    false
}

pub(crate) enum GossipFilterDirection {
    Ingress,
    EgressPush,
    EgressPullResponse,
}

/// Minimum number of staked nodes for enforcing stakes in gossip.
const MIN_NUM_STAKED_NODES: usize = 500;

/// Minimum stake that a node should have so that all its CRDS values are
/// propagated through gossip (below this only subset of CRDS is propagated).
pub(crate) const MIN_STAKE_FOR_GOSSIP: u64 = solana_native_token::LAMPORTS_PER_SOL;

/// Returns false if the CRDS value should be discarded.
/// `direction` controls whether we are looking at
/// incoming packet (via Push or PullResponse) or
/// we are about to make a packet.
/// `is_full_alpenglow_epoch` tells the filter if Alpenglow migration is complete.
#[inline]
#[must_use]
pub(crate) fn should_retain_crds_value(
    value: &CrdsValue,
    stakes: &HashMap<Pubkey, u64>,
    direction: GossipFilterDirection,
    is_full_alpenglow_epoch: bool,
) -> bool {
    if is_gossip_minimal_mode() {
        return match value.data() {
            CrdsData::ContactInfo(node) => node.has_consistent_udp_ip(),
            _ => false,
        };
    }

    let retain_if_staked = || {
        stakes.len() < MIN_NUM_STAKED_NODES || {
            let stake = stakes.get(&value.pubkey()).copied();
            stake.unwrap_or_default() >= MIN_STAKE_FOR_GOSSIP
        }
    };

    use GossipFilterDirection::*;
    match value.data() {
        CrdsData::ContactInfo(node) => node.has_consistent_udp_ip(),
        // Unstaked nodes can still serve snapshots.
        CrdsData::SnapshotHashes(_) => true,
        // Disabled once Alpenglow is active.
        CrdsData::DuplicateShred(_, _) => !is_full_alpenglow_epoch && retain_if_staked(),
        // Consensus related messages only allowed for staked nodes
        CrdsData::LowestSlot(0, _)
        | CrdsData::RestartHeaviestFork(_)
        | CrdsData::RestartLastVotedForkSlots(_) => retain_if_staked(),
        CrdsData::EpochSlots(_, _) if is_full_alpenglow_epoch => false,
        // Unstaked nodes can technically send EpochSlots, but we do not want them
        // eating gossip bandwidth.
        CrdsData::EpochSlots(_, _) => {
            match direction {
                // always store if we have received them
                // to avoid getting them again in PullResponses
                Ingress => true,
                // only forward if the origin is staked
                EgressPush | EgressPullResponse => retain_if_staked(),
            }
        }
        CrdsData::Vote(_, _) if is_full_alpenglow_epoch => false,
        CrdsData::Vote(_, _) => match direction {
            Ingress | EgressPush => true,
            EgressPullResponse => retain_if_staked(),
        },
        // Fully deprecated messages
        CrdsData::AccountsHashes(_) => false,
        CrdsData::LegacyContactInfo(_) => false,
        CrdsData::LegacySnapshotHashes(_) => false,
        CrdsData::LegacyVersion(_) => false,
        CrdsData::LowestSlot(1.., _) => false,
        CrdsData::NodeInstance(_) => false,
        CrdsData::Version(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_pre_drop_gossip_packet() {
        set_gossip_minimal_mode(true);

        // Tag 0: PullRequest -> drop
        let pull_request_bytes = 0u32.to_le_bytes();
        assert!(should_pre_drop_gossip_packet(&pull_request_bytes));

        // Tag 2: PushMessage -> drop
        let push_msg_bytes = 2u32.to_le_bytes();
        assert!(should_pre_drop_gossip_packet(&push_msg_bytes));

        // Tag 3: PruneMessage -> drop
        let prune_msg_bytes = 3u32.to_le_bytes();
        assert!(should_pre_drop_gossip_packet(&prune_msg_bytes));

        // Short packet (< 4 bytes) -> drop
        assert!(should_pre_drop_gossip_packet(&[2, 0]));

        // Tag 1: PullResponse -> keep
        let pull_resp_bytes = 1u32.to_le_bytes();
        assert!(!should_pre_drop_gossip_packet(&pull_resp_bytes));

        // Tag 4: PingMessage -> keep
        let ping_bytes = 4u32.to_le_bytes();
        assert!(!should_pre_drop_gossip_packet(&ping_bytes));

        // Tag 5: PongMessage -> keep
        let pong_bytes = 5u32.to_le_bytes();
        assert!(!should_pre_drop_gossip_packet(&pong_bytes));

        // When minimal mode is disabled, never pre-drop
        set_gossip_minimal_mode(false);
        assert!(!should_pre_drop_gossip_packet(&push_msg_bytes));
        assert!(!should_pre_drop_gossip_packet(&pull_request_bytes));
        assert!(!should_pre_drop_gossip_packet(&prune_msg_bytes));
    }
}

