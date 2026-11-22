//! SMP (Secure Message Protocol) relay — ADR-0007.
//! Kademlia-based routing of encrypted envelopes to the closest peers to
//! `recipient_node_id`, with pending-envelope storage and TTL.
//! Pure logic — no I/O, compiles to wasm32 unchanged.

use crate::error::NexxusError;
use crate::types::NodeId;
use crate::wire::payloads::SmpEnvelope;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// A pending (undelivered) SMP envelope with expiry.
#[derive(Debug, Clone)]
struct PendingEnvelope {
    env: SmpEnvelope,
    expires_at: Instant,
}

pub const MAX_PENDING_PER_RECIPIENT: usize = 256;
pub const MAX_TOTAL_PENDING_ENVELOPES: usize = 10_000;

/// Full 256-bit XOR distance between two NodeIds (M-20 fix: preserves all 32 bytes).
/// [u8; 32] implements lexicographical ordering (most significant byte first).
fn xor_distance(a: &NodeId, b: &NodeId) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = a.0[i] ^ b.0[i];
    }
    out
}

/// SMP relay table: maps recipient node_id -> pending envelopes.
/// Peers are tracked by their NodeId; routing picks the closest known peer
/// to the target recipient.
#[derive(Debug, Clone, Default)]
pub struct SmpRelay {
    /// recipient_node_id -> pending envelopes (ordered by expiry)
    pending: BTreeMap<NodeId, Vec<PendingEnvelope>>,
    /// known peers: node_id -> last seen
    peers: BTreeMap<NodeId, Instant>,
    ttl: Duration,
}

impl SmpRelay {
    pub fn new(ttl: Duration) -> Self {
        Self {
            pending: BTreeMap::new(),
            peers: BTreeMap::new(),
            ttl,
        }
    }

    /// Register that we know about a peer (used for routing decisions).
    pub fn register_peer(&mut self, node_id: NodeId) {
        self.peers.insert(node_id, Instant::now());
    }

    /// Store an envelope for later delivery to `recipient_node_id`.
    /// Returns error if the envelope is too large or if total pending limits are exceeded (M-20).
    pub fn store_envelope(&mut self, env: SmpEnvelope) -> Result<(), NexxusError> {
        if env.ciphertext.len() > (1 << 20) {
            return Err(NexxusError::Io("smp envelope too large".into()));
        }

        // Enforce total pending limit
        if self.pending_count() >= MAX_TOTAL_PENDING_ENVELOPES {
            self.gc();
            if self.pending_count() >= MAX_TOTAL_PENDING_ENVELOPES {
                return Err(NexxusError::Io("smp relay total pending capacity exceeded".into()));
            }
        }

        let recipient = NodeId(env.recipient_node_id);
        let list = self.pending.entry(recipient).or_default();

        // Enforce per-recipient limit
        if list.len() >= MAX_PENDING_PER_RECIPIENT {
            let now = Instant::now();
            list.retain(|p| p.expires_at > now);
            if list.len() >= MAX_PENDING_PER_RECIPIENT {
                // Drop oldest
                list.remove(0);
            }
        }

        let pending = PendingEnvelope {
            env,
            expires_at: Instant::now() + self.ttl,
        };
        list.push(pending);
        Ok(())
    }

    /// Retrieve and remove all non-expired envelopes for a recipient.
    /// Expired envelopes are dropped.
    pub fn take_pending(&mut self, recipient: NodeId) -> Vec<SmpEnvelope> {
        let now = Instant::now();
        let mut result = Vec::new();
        if let Some(envs) = self.pending.get_mut(&recipient) {
            envs.retain(|p| p.expires_at > now);
            result.extend(envs.drain(..).map(|p| p.env));
            if envs.is_empty() {
                self.pending.remove(&recipient);
            }
        }
        result
    }

    /// Return the `count` closest known peers to `target` (full 256-bit Kademlia lookup).
    pub fn closest_peers(&self, target: &NodeId, count: usize) -> Vec<NodeId> {
        let mut peers: Vec<(NodeId, [u8; 32])> = self
            .peers
            .keys()
            .filter(|pid| **pid != *target)
            .map(|pid| (*pid, xor_distance(pid, target)))
            .collect();
        peers.sort_by_key(|a| a.1);
        peers.into_iter().take(count).map(|(pid, _)| pid).collect()
    }

    /// Purge all expired envelopes (call periodically).
    pub fn gc(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, envs| {
            envs.retain(|p| p.expires_at > now);
            !envs.is_empty()
        });
    }

    /// Number of pending envelopes across all recipients.
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }

    /// Number of known peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

/// Specification §7: Contact exchange standard for QR codes and deep links.
/// Format: `nexxus://contact?addr=<Bech32Address>&onion=<Torv3OnionAddress>&name=<ContactAlias>&fp=<PublicKeyFingerprint>`
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContactLink {
    pub addr: String,
    pub onion: String,
    pub name: String,
    pub fingerprint: String,
}

impl ContactLink {
    pub fn new(
        addr: impl Into<String>,
        onion: impl Into<String>,
        name: impl Into<String>,
        fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            addr: addr.into(),
            onion: onion.into(),
            name: name.into(),
            fingerprint: fingerprint.into(),
        }
    }

    /// Serializes contact link to canonical `nexxus://contact?...` URI.
    pub fn to_uri(&self) -> String {
        format!(
            "nexxus://contact?addr={}&onion={}&name={}&fp={}",
            url_encode(&self.addr),
            url_encode(&self.onion),
            url_encode(&self.name),
            url_encode(&self.fingerprint),
        )
    }

    /// Parses deep link URI from standard or legacy format.
    pub fn from_uri(uri: &str) -> Result<Self, NexxusError> {
        let query = if let Some(q) = uri.strip_prefix("nexxus://contact?") {
            q
        } else if let Some(q) = uri.strip_prefix("nexxus://add/") {
            q
        } else {
            return Err(NexxusError::Io(
                "invalid URI scheme: expected nexxus://contact?...".into(),
            ));
        };

        let mut addr = None;
        let mut onion = None;
        let mut name = None;
        let mut fp = None;

        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k {
                    "addr" => addr = Some(url_decode(v)),
                    "onion" => onion = Some(url_decode(v)),
                    "name" => name = Some(url_decode(v)),
                    "fp" | "fingerprint" => fp = Some(url_decode(v)),
                    _ => {}
                }
            }
        }

        let addr = addr.ok_or_else(|| NexxusError::Io("missing 'addr' in contact link".into()))?;
        let onion = onion.ok_or_else(|| NexxusError::Io("missing 'onion' in contact link".into()))?;
        let name = name.unwrap_or_else(|| "Anonymous".into());
        let fingerprint = fp.unwrap_or_default();

        if !onion.ends_with(".onion") {
            return Err(NexxusError::Io(
                "invalid onion address in contact link (must end with .onion)".into(),
            ));
        }

        Ok(Self {
            addr,
            onion,
            name,
            fingerprint,
        })
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        match b {
            b'+' => bytes.push(b' '),
            b'%' => {
                let h1 = chars.next().unwrap_or(b'0') as char;
                let h2 = chars.next().unwrap_or(b'0') as char;
                let hex_str = format!("{h1}{h2}");
                if let Ok(val) = u8::from_str_radix(&hex_str, 16) {
                    bytes.push(val);
                } else {
                    bytes.push(b'%');
                }
            }
            _ => bytes.push(b),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::payloads::{NodeAnnounce, Payload};
    use std::time::Duration;

    #[test]
    fn store_and_take_pending() {
        let mut relay = SmpRelay::new(Duration::from_secs(60));
        let payload = b"hello smp";
        let mut secret = [0u8; 32];
        use rand::rngs::OsRng;
        use rand::Rng;
        OsRng.fill(&mut secret);
        let pub_point = crate::wire::payloads::X25519_BASEPOINT.mul_clamped(secret);
        let (_ephemeral, env) = SmpEnvelope::seal(
            [1u8; 32],
            [2u8; 32],
            &pub_point,
            payload,
        );

        relay.store_envelope(env.clone()).unwrap();
        assert_eq!(relay.pending_count(), 1);

        let taken = relay.take_pending(NodeId([2u8; 32]));
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].ciphertext, env.ciphertext);
        assert_eq!(relay.pending_count(), 0);
    }

    #[test]
    fn expired_envelopes_dropped() {
        let mut relay = SmpRelay::new(Duration::from_nanos(1));
        let payload = b"expires immediately";
        let mut secret = [0u8; 32];
        use rand::rngs::OsRng;
        use rand::Rng;
        OsRng.fill(&mut secret);
        let pub_point = crate::wire::payloads::X25519_BASEPOINT.mul_clamped(secret);
        let (_ephemeral, env) = SmpEnvelope::seal(
            [1u8; 32],
            [2u8; 32],
            &pub_point,
            payload,
        );
        relay.store_envelope(env).unwrap();
        // force expiry by sleeping a tiny bit
        std::thread::sleep(Duration::from_nanos(10));
        let taken = relay.take_pending(NodeId([2u8; 32]));
        assert_eq!(taken.len(), 0); // expired
        assert_eq!(relay.pending_count(), 0);
    }

    #[test]
    fn closest_peers_by_xor() {
        let mut relay = SmpRelay::new(Duration::from_secs(60));
        let target = NodeId([0x00u8; 32]);
        relay.register_peer(NodeId([0xFFu8; 32])); // far
        relay.register_peer(NodeId([0x01u8; 32])); // close
        relay.register_peer(NodeId([0x80u8; 32])); // medium
        let closest = relay.closest_peers(&target, 2);
        assert_eq!(closest.len(), 2);
        assert_eq!(closest[0], NodeId([0x01u8; 32]));
        assert_eq!(closest[1], NodeId([0x80u8; 32]));
    }

    #[test]
    fn xor_distance_checks_upper_16_bytes_first() {
        let mut relay = SmpRelay::new(Duration::from_secs(60));
        let target = NodeId([0x00u8; 32]);
        // peer_a has difference in byte 0 (most significant byte): far
        let mut a = [0x00u8; 32];
        a[0] = 0x80;
        // peer_b has difference in byte 31 (least significant byte): very close
        let mut b = [0x00u8; 32];
        b[31] = 0x80;
        relay.register_peer(NodeId(a));
        relay.register_peer(NodeId(b));
        let closest = relay.closest_peers(&target, 2);
        assert_eq!(closest[0], NodeId(b), "M-20: peer differing in byte 31 must be closer than peer differing in byte 0");
        assert_eq!(closest[1], NodeId(a));
    }

    #[test]
    fn closest_peers_excludes_target() {
        let mut relay = SmpRelay::new(Duration::from_secs(60));
        let me = NodeId([0x42u8; 32]);
        relay.register_peer(me);
        let closest = relay.closest_peers(&me, 5);
        assert_eq!(closest.len(), 0); // self excluded
    }

    #[test]
    fn node_announce_roundtrip_in_smp() {
        let announce = NodeAnnounce {
            node_id: [0x42u8; 32],
            pubkey: [0x99u8; 32],
            ip: 0x0a000001,
            port: 4001,
            session_nonce: [0x11u8; 32],
        };
        let dec = NodeAnnounce::decode(&announce.encode()).unwrap();
        assert_eq!(dec, announce);
    }

    #[test]
    fn contact_link_roundtrip_and_uri_validation() {
        // Specification §7: URI standard test
        let link = ContactLink::new(
            "nx1q9f7x4a2e8w3u1s0d6c8b5m2k4p7z9l3j5h",
            "25njqamcweflpvkl73j4szahhihoc4xt3ktcgjnpaingr5yhkenjpzad.onion",
            "Alice Node",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        );

        let uri = link.to_uri();
        assert!(uri.starts_with("nexxus://contact?"));
        assert!(uri.contains("addr=nx1q"));
        assert!(uri.contains("onion=25njqamc"));

        let parsed = ContactLink::from_uri(&uri).unwrap();
        assert_eq!(parsed, link);

        // Invalid scheme rejection
        assert!(ContactLink::from_uri("https://example.com").is_err());

        // Invalid onion address rejection
        assert!(ContactLink::from_uri("nexxus://contact?addr=abc&onion=not-onion&name=Bob").is_err());
    }
}
