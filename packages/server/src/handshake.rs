//! Proof of the session token without revealing it (audit #18).
//!
//! An ssh session starts `moonkale-server` on a random port of the host and
//! forwards it. Another user of that host could listen on the port first;
//! a client that sends its bearer token to whatever answers would give the
//! token away. So the client first sends a random challenge to
//! `POST /moonkale-proof` (public, no token), and talks to the server only if
//! the answer is `HMAC-SHA256(token, challenge)` — which only a server that
//! was handed the token can compute.

use sha2::{Digest, Sha256};

/// The public route that answers challenges.
pub const PATH: &str = "/moonkale-proof";

/// `HMAC-SHA256(token, challenge)`, hex.
pub fn proof(token: &str, challenge: &str) -> String {
    const BLOCK: usize = 64;
    let mut key = token.as_bytes().to_vec();
    if key.len() > BLOCK {
        key = Sha256::digest(&key).to_vec();
    }
    key.resize(BLOCK, 0);
    let pad = |b: u8| key.iter().map(|k| k ^ b).collect::<Vec<u8>>();
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(challenge.as_bytes())
        .finalize();
    let outer = Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize();
    outer.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh random challenge.
pub fn challenge() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS randomness");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 2.
    #[test]
    fn hmac_sha256_matches_the_rfc() {
        assert_eq!(
            proof("Jefe", "what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_ne!(proof("a", "x"), proof("b", "x"));
        assert_eq!(challenge().len(), 32);
    }
}
