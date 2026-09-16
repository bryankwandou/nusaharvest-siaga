//! Roster merkle tree.
//!
//! leaf = sha256("NH1" || campaign_32 || claimant_32 || index_u32_le)
//! node = sha256("NHN" || min(a,b) || max(a,b))   (sorted pairs, no position bits)
//!
//! The distinct "NH1"/"NHN" prefixes stop an inner node being replayed as a leaf.

use solana_sha256_hasher::hashv;

pub const MAX_PROOF_LEN: usize = 24;

pub fn leaf(campaign: &[u8; 32], claimant: &[u8; 32], index: u32) -> [u8; 32] {
    hashv(&[b"NH1", campaign, claimant, &index.to_le_bytes()]).to_bytes()
}

pub fn node(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (l, r) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[b"NHN", l, r]).to_bytes()
}

/// `proof` is a concatenation of 32-byte siblings.
pub fn verify(root: &[u8; 32], leaf: [u8; 32], proof: &[u8]) -> bool {
    if proof.len() % 32 != 0 || proof.len() / 32 > MAX_PROOF_LEN {
        return false;
    }
    let mut acc = leaf;
    for chunk in proof.chunks_exact(32) {
        let mut sib = [0u8; 32];
        sib.copy_from_slice(chunk);
        acc = node(&acc, &sib);
    }
    &acc == root
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Off-chain helper: build root and per-leaf proofs (odd node promoted).
    pub fn build(leaves: &[[u8; 32]]) -> ([u8; 32], Vec<Vec<u8>>) {
        let n = leaves.len();
        let mut proofs = vec![Vec::new(); n];
        let mut pos: Vec<usize> = (0..n).collect();
        let mut level = leaves.to_vec();
        while level.len() > 1 {
            let mut next = Vec::new();
            for i in (0..level.len()).step_by(2) {
                if i + 1 < level.len() {
                    next.push(node(&level[i], &level[i + 1]));
                } else {
                    next.push(level[i]);
                }
            }
            for (leaf_i, p) in pos.iter_mut().enumerate() {
                let sib = *p ^ 1;
                if sib < level.len() {
                    proofs[leaf_i].extend_from_slice(&level[sib]);
                }
                *p /= 2;
            }
            level = next;
        }
        (level[0], proofs)
    }

    #[test]
    fn proofs_verify_and_tamper_fails() {
        let camp = [7u8; 32];
        let leaves: Vec<_> = (0..5u32).map(|i| leaf(&camp, &[i as u8 + 1; 32], i)).collect();
        let (root, proofs) = build(&leaves);
        for i in 0..5 {
            assert!(verify(&root, leaves[i], &proofs[i]));
        }
        // wrong claimant
        assert!(!verify(&root, leaf(&camp, &[99; 32], 0), &proofs[0]));
        // wrong index
        assert!(!verify(&root, leaf(&camp, &[1; 32], 1), &proofs[0]));
        // malformed proof
        assert!(!verify(&root, leaves[0], &proofs[0][..31]));
    }
}
