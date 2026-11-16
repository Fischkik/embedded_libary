//! Bluetooth Mesh: key derivation, the encrypted network PDU, and a flooding simulation.
//!
//! References are to the Mesh Protocol specification 1.1. The tests check this code
//! against the sample data in its section 8.

use crate::crypto::{aes_e, ccm_decrypt, ccm_encrypt, cmac};
use crate::rng::Rng;

pub type Key = [u8; 16];

/// Salt generation function `s1` (3.9.2.4).
pub fn s1(m: &[u8]) -> Key {
    cmac(&[0; 16], m)
}

/// Key derivation function `k1` (3.9.2.5).
pub fn k1(n: &[u8], salt: &Key, p: &[u8]) -> Key {
    cmac(&cmac(salt, n), p)
}

/// What a NetKey turns into: the pieces actually used on air.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetKeyMaterial {
    /// 7-bit network identifier, sent in the clear so receivers can pick the right key.
    pub nid: u8,
    pub encryption_key: Key,
    pub privacy_key: Key,
}

/// Key derivation function `k2` (3.9.2.6). `p = [0]` gives the normal flooding keys.
pub fn k2(net_key: &Key, p: &[u8]) -> NetKeyMaterial {
    let t = cmac(&s1(b"smk2"), net_key);
    let step = |prev: &[u8], n: u8| {
        let mut m = prev.to_vec();
        m.extend_from_slice(p);
        m.push(n);
        cmac(&t, &m)
    };
    let t1 = step(&[], 1);
    let t2 = step(&t1, 2);
    let t3 = step(&t2, 3);
    NetKeyMaterial { nid: t1[15] & 0x7F, encryption_key: t2, privacy_key: t3 }
}

/// `k3` (3.9.2.7): the public 8-byte Network ID of a NetKey.
pub fn k3(net_key: &Key) -> [u8; 8] {
    let t = cmac(&s1(b"smk3"), net_key);
    let mut out = [0u8; 8];
    out.copy_from_slice(&cmac(&t, b"id64\x01")[8..]);
    out
}

/// `k4` (3.9.2.8): the 6-bit application key identifier (AID) of an AppKey.
pub fn k4(app_key: &Key) -> u8 {
    let t = cmac(&s1(b"smk4"), app_key);
    cmac(&t, b"id6\x01")[15] & 0x3F
}

/// The kind of a 16-bit mesh address (3.4.2).
pub fn address_kind(addr: u16) -> &'static str {
    match addr {
        0x0000 => "unassigned",
        0x0001..=0x7FFF => "unicast (one element of one node)",
        0x8000..=0xBFFF => "virtual (stands for a label UUID)",
        0xFFFC => "group: all proxies",
        0xFFFD => "group: all friends",
        0xFFFE => "group: all relays",
        0xFFFF => "group: all nodes",
        _ => "group",
    }
}

/// What a TTL value means (3.4.4.4).
pub fn ttl_meaning(ttl: u8) -> &'static str {
    match ttl {
        0 => "was not relayed and will not be relayed (single hop)",
        1 => "may have been relayed, will not be relayed again",
        2..=126 => "may have been relayed and can be relayed again",
        _ => "has not been relayed yet and can be relayed",
    }
}

/// The readable fields of a network PDU (3.4.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPdu {
    /// true = control message (64-bit NetMIC), false = access message (32-bit NetMIC).
    pub ctl: bool,
    pub ttl: u8,
    /// 24-bit sequence number.
    pub seq: u32,
    pub src: u16,
    pub dst: u16,
    pub transport_pdu: Vec<u8>,
}

/// Every intermediate value of network-layer encryption, for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkSteps {
    pub ivi_nid: u8,
    pub nonce: [u8; 13],
    /// `CTL||TTL, SEQ, SRC` before obfuscation.
    pub header_plain: [u8; 6],
    /// Encrypted `DST || TransportPDU`, without the NetMIC.
    pub encrypted: Vec<u8>,
    pub net_mic: Vec<u8>,
    pub privacy_plaintext: [u8; 16],
    pub pecb: [u8; 6],
    pub header_obfuscated: [u8; 6],
    /// The finished network PDU as it is handed to the bearer.
    pub pdu: Vec<u8>,
}

fn network_nonce(ctl_ttl: u8, seq: u32, src: u16, iv_index: u32) -> [u8; 13] {
    let mut n = [0u8; 13];
    n[1] = ctl_ttl;
    n[2..5].copy_from_slice(&seq.to_be_bytes()[1..]);
    n[5..7].copy_from_slice(&src.to_be_bytes());
    n[9..].copy_from_slice(&iv_index.to_be_bytes());
    n
}

fn pecb(privacy_key: &Key, iv_index: u32, privacy_random: &[u8]) -> ([u8; 16], [u8; 6]) {
    let mut plain = [0u8; 16];
    plain[5..9].copy_from_slice(&iv_index.to_be_bytes());
    let n = privacy_random.len().min(7);
    plain[9..9 + n].copy_from_slice(&privacy_random[..n]);
    let e = aes_e(privacy_key, &plain);
    (plain, [e[0], e[1], e[2], e[3], e[4], e[5]])
}

/// Encrypt and obfuscate a network PDU (3.9.6 and 3.9.7).
pub fn encode_network(pdu: &NetworkPdu, keys: &NetKeyMaterial, iv_index: u32) -> NetworkSteps {
    let ivi_nid = ((iv_index & 1) as u8) << 7 | (keys.nid & 0x7F);
    let ctl_ttl = (pdu.ctl as u8) << 7 | (pdu.ttl & 0x7F);
    let seq = pdu.seq & 0xFF_FFFF;
    let nonce = network_nonce(ctl_ttl, seq, pdu.src, iv_index);
    let mut plain = pdu.dst.to_be_bytes().to_vec();
    plain.extend_from_slice(&pdu.transport_pdu);
    let mic_len = if pdu.ctl { 8 } else { 4 };
    let mut encrypted = ccm_encrypt(&keys.encryption_key, &nonce, &[], &plain, mic_len);
    let net_mic = encrypted.split_off(encrypted.len() - mic_len);

    let mut tail = encrypted.clone();
    tail.extend_from_slice(&net_mic);
    let (privacy_plaintext, pecb) = pecb(&keys.privacy_key, iv_index, &tail);
    let mut header_plain = [0u8; 6];
    header_plain[0] = ctl_ttl;
    header_plain[1..4].copy_from_slice(&seq.to_be_bytes()[1..]);
    header_plain[4..].copy_from_slice(&pdu.src.to_be_bytes());
    let mut header_obfuscated = header_plain;
    header_obfuscated.iter_mut().zip(pecb).for_each(|(h, p)| *h ^= p);

    let mut out = vec![ivi_nid];
    out.extend_from_slice(&header_obfuscated);
    out.extend_from_slice(&tail);
    NetworkSteps { ivi_nid, nonce, header_plain, encrypted, net_mic, privacy_plaintext, pecb, header_obfuscated, pdu: out }
}

/// Why a received network PDU was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    /// The NID in the first byte does not match our key: the packet is for another network.
    WrongNid { got: u8, ours: u8 },
    /// Decryption worked mechanically but the NetMIC is wrong: wrong key, wrong IV index
    /// or a changed packet.
    BadNetMic,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "too short to be a network PDU (minimum 14 bytes)"),
            Self::WrongNid { got, ours } => {
                write!(f, "NID is 0x{got:02x} but our key has NID 0x{ours:02x}: another network, ignored")
            }
            Self::BadNetMic => write!(f, "NetMIC check failed: wrong key or IV index, or the packet was changed"),
        }
    }
}

/// De-obfuscate, decrypt and authenticate a received network PDU.
pub fn decode_network(bytes: &[u8], keys: &NetKeyMaterial, iv_index: u32) -> Result<NetworkPdu, DecodeError> {
    if bytes.len() < 1 + 6 + 2 + 1 + 4 {
        return Err(DecodeError::TooShort);
    }
    if bytes[0] & 0x7F != keys.nid {
        return Err(DecodeError::WrongNid { got: bytes[0] & 0x7F, ours: keys.nid });
    }
    let (_, pecb) = pecb(&keys.privacy_key, iv_index, &bytes[7..]);
    let mut header = [0u8; 6];
    for i in 0..6 {
        header[i] = bytes[1 + i] ^ pecb[i];
    }
    let ctl = header[0] & 0x80 != 0;
    let seq = u32::from_be_bytes([0, header[1], header[2], header[3]]);
    let src = u16::from_be_bytes([header[4], header[5]]);
    let nonce = network_nonce(header[0], seq, src, iv_index);
    let mic_len = if ctl { 8 } else { 4 };
    let plain = ccm_decrypt(&keys.encryption_key, &nonce, &[], &bytes[7..], mic_len).ok_or(DecodeError::BadNetMic)?;
    if plain.len() < 3 {
        return Err(DecodeError::TooShort);
    }
    Ok(NetworkPdu {
        ctl,
        ttl: header[0] & 0x7F,
        seq,
        src,
        dst: u16::from_be_bytes([plain[0], plain[1]]),
        transport_pdu: plain[2..].to_vec(),
    })
}

/// Every intermediate value of application-layer (upper transport) encryption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessSteps {
    pub aid: u8,
    pub nonce: [u8; 13],
    pub encrypted: Vec<u8>,
    pub trans_mic: Vec<u8>,
    /// Lower transport header: SEG=0, AKF=1, AID.
    pub header: u8,
    /// Header + encrypted access message + TransMIC: what the network layer carries.
    pub lower_transport_pdu: Vec<u8>,
}

/// Encrypt an access message with an AppKey and wrap it as an unsegmented lower
/// transport PDU (3.5.2.1, 3.9.5.2). Fits access messages up to 11 bytes.
pub fn encode_access(access: &[u8], app_key: &Key, seq: u32, src: u16, dst: u16, iv_index: u32) -> AccessSteps {
    let aid = k4(app_key);
    let mut nonce = [0u8; 13];
    nonce[0] = 0x01;
    nonce[2..5].copy_from_slice(&(seq & 0xFF_FFFF).to_be_bytes()[1..]);
    nonce[5..7].copy_from_slice(&src.to_be_bytes());
    nonce[7..9].copy_from_slice(&dst.to_be_bytes());
    nonce[9..].copy_from_slice(&iv_index.to_be_bytes());
    let mut encrypted = ccm_encrypt(app_key, &nonce, &[], access, 4);
    let trans_mic = encrypted.split_off(encrypted.len() - 4);
    let header = 0x40 | aid;
    let mut lower = vec![header];
    lower.extend_from_slice(&encrypted);
    lower.extend_from_slice(&trans_mic);
    AccessSteps { aid, nonce, encrypted, trans_mic, header, lower_transport_pdu: lower }
}

/// Reverse of [`encode_access`]. `None` means the AppKey does not fit (wrong AID or TransMIC).
pub fn decode_access(lower: &[u8], app_key: &Key, seq: u32, src: u16, dst: u16, iv_index: u32) -> Option<Vec<u8>> {
    let (&header, rest) = lower.split_first()?;
    if header & 0x80 != 0 || header & 0x40 == 0 || header & 0x3F != k4(app_key) {
        return None;
    }
    let mut nonce = [0u8; 13];
    nonce[0] = 0x01;
    nonce[2..5].copy_from_slice(&(seq & 0xFF_FFFF).to_be_bytes()[1..]);
    nonce[5..7].copy_from_slice(&src.to_be_bytes());
    nonce[7..9].copy_from_slice(&dst.to_be_bytes());
    nonce[9..].copy_from_slice(&iv_index.to_be_bytes());
    ccm_decrypt(app_key, &nonce, &[], rest, 4)
}

/// Generic OnOff Set Unacknowledged (Mesh Model 3.2.1.3): opcode 0x82 0x03, OnOff, TID.
pub fn generic_onoff_set_unack(on: bool, tid: u8) -> Vec<u8> {
    vec![0x82, 0x03, on as u8, tid]
}

/// Describe an access message in words (only the Generic OnOff messages are known).
pub fn describe_access(access: &[u8]) -> String {
    match access {
        [0x82, 0x01] => "Generic OnOff Get".into(),
        [0x82, op @ (0x02 | 0x03), onoff, tid, ..] => format!(
            "Generic OnOff Set{}: turn {} (transaction {tid})",
            if *op == 3 { " Unacknowledged" } else { "" },
            if *onoff == 1 { "ON" } else { "OFF" }
        ),
        [0x82, 0x04, onoff, ..] => format!("Generic OnOff Status: is {}", if *onoff == 1 { "ON" } else { "OFF" }),
        [] => "empty".into(),
        _ => "an access message this demo does not decode".into(),
    }
}

// ---------------------------------------------------------------------------------
// Managed flooding simulation
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloodNode {
    pub x: f64,
    pub y: f64,
    /// Whether the Relay feature is switched on for this node.
    pub relay: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloodParams {
    /// Radio range in the same unit as node positions.
    pub range: f64,
    /// TTL the source puts into the message.
    pub ttl: u8,
    pub src: usize,
    /// Destination node, or `None` for "all nodes".
    pub dst: Option<usize>,
    /// If false, nodes relay everything they hear, every time (to show why the
    /// network message cache exists).
    pub use_cache: bool,
    pub seed: u64,
}

/// What a node did with a packet it heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    /// Already in the network message cache: dropped at once.
    Duplicate,
    /// New, and this node is the destination.
    Delivered,
    /// New, will be sent on with TTL - 1.
    WillRelay,
    /// New, but TTL is too low to relay.
    TtlExhausted,
    /// New, but this node is not a relay.
    NotARelay,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transmission {
    pub t_ms: f64,
    pub from: usize,
    pub ttl: u8,
    pub heard: Vec<(usize, Reaction)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FloodResult {
    pub transmissions: Vec<Transmission>,
    /// When the destination first got the message (or the last node, for "all nodes").
    pub delivered_ms: Option<f64>,
    /// Nodes that received the message at least once.
    pub reached: Vec<bool>,
    /// True if the simulation was stopped because it ran away (cache off).
    pub truncated: bool,
}

/// Simulate one message travelling through a mesh by managed flooding (3.4.6).
///
/// Simplifications: a packet is heard by every node within `range` and never lost;
/// each node sends it once (real nodes repeat it a few times on three channels);
/// relays wait a random 5..25 ms before sending.
pub fn flood(nodes: &[FloodNode], p: &FloodParams) -> FloodResult {
    const MAX_TX: usize = 400;
    let mut rng = Rng::new(p.seed);
    let mut result = FloodResult { reached: vec![false; nodes.len()], ..Default::default() };
    if p.src >= nodes.len() {
        return result;
    }
    let mut cached = vec![false; nodes.len()];
    cached[p.src] = true;
    // Pending transmissions: (time, node, ttl).
    let mut queue: Vec<(f64, usize, u8)> = vec![(0.0, p.src, p.ttl)];
    while !queue.is_empty() {
        if result.transmissions.len() >= MAX_TX {
            result.truncated = true;
            break;
        }
        let next = (0..queue.len()).min_by(|&a, &b| queue[a].0.total_cmp(&queue[b].0)).unwrap_or(0);
        let (t, from, ttl) = queue.remove(next);
        let mut heard = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            if i == from || ((n.x - nodes[from].x).powi(2) + (n.y - nodes[from].y).powi(2)).sqrt() > p.range {
                continue;
            }
            let reaction = if cached[i] && p.use_cache {
                Reaction::Duplicate
            } else {
                let first_time = !cached[i];
                cached[i] = true;
                result.reached[i] = true;
                if p.dst == Some(i) {
                    if first_time {
                        result.delivered_ms.get_or_insert(t);
                    }
                    Reaction::Delivered
                } else if !n.relay {
                    Reaction::NotARelay
                } else if ttl < 2 {
                    Reaction::TtlExhausted
                } else {
                    queue.push((t + rng.range(5.0, 25.0), i, ttl - 1));
                    Reaction::WillRelay
                }
            };
            heard.push((i, reaction));
        }
        result.transmissions.push(Transmission { t_ms: t, from, ttl, heard });
    }
    if p.dst.is_none() {
        let all = result.reached.iter().enumerate().all(|(i, r)| *r || i == p.src);
        if all {
            result.delivered_ms = result.transmissions.last().map(|t| t.t_ms);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::unhex;

    fn key(h: &str) -> Key {
        unhex(h).unwrap().try_into().unwrap()
    }

    // Mesh Protocol 1.1 section 8.1 sample keys.
    const NET_KEY_81: &str = "f7a2a44f8e8a8029064f173ddc1e2b00";
    const APP_KEY_81: &str = "3216d1509884b533248541792b877f98";
    // Section 8.2 / 8.3 keys.
    const NET_KEY: &str = "7dd7364cd842ad18c17c2b820c84c3d6";
    const APP_KEY: &str = "63964771734fbd76e3b40519d1d94a48";

    #[test]
    fn s1_and_k1_match_spec() {
        assert_eq!(s1(b"test"), key("b73cefbd641ef2ea598c2b6efb62f79c"));
        let out = k1(&unhex(APP_KEY_81).unwrap(), &key("2ba14ffa0df84a2831938d57d276cab4"), &unhex("5a09d60797eeb4478aada59db3352a0d").unwrap());
        assert_eq!(out, key("f6ed15a8934afbe7d83e8dcb57fcf5d7"));
    }

    #[test]
    fn k2_k3_k4_match_spec() {
        let m = k2(&key(NET_KEY_81), &[0]);
        assert_eq!(m.nid, 0x7f);
        assert_eq!(m.encryption_key, key("9f589181a0f50de73c8070c7a6d27f46"));
        assert_eq!(m.privacy_key, key("4c715bd4a64b938f99b453351653124f"));
        let friend = k2(&key(NET_KEY_81), &unhex("010203040506070809").unwrap());
        assert_eq!(friend.nid, 0x73);
        assert_eq!(friend.encryption_key, key("11efec0642774992510fb5929646df49"));
        assert_eq!(k3(&key(NET_KEY_81)).to_vec(), unhex("ff046958233db014").unwrap());
        assert_eq!(k4(&key(APP_KEY_81)), 0x38);

        let m2 = k2(&key(NET_KEY), &[0]);
        assert_eq!(m2.nid, 0x68);
        assert_eq!(m2.encryption_key, key("0953fa93e7caac9638f58820220a398e"));
        assert_eq!(m2.privacy_key, key("8b84eedec100067d670971dd2aa700cf"));
        assert_eq!(k3(&key(NET_KEY)).to_vec(), unhex("3ecaff672f673370").unwrap());
        assert_eq!(k4(&key(APP_KEY)), 0x26);
    }

    #[test]
    fn message_1_control_pdu_matches_spec() {
        let keys = k2(&key(NET_KEY), &[0]);
        let pdu = NetworkPdu { ctl: true, ttl: 0, seq: 1, src: 0x1201, dst: 0xfffd, transport_pdu: unhex("034b50057e400000010000").unwrap() };
        let s = encode_network(&pdu, &keys, 0x1234_5678);
        assert_eq!(s.nonce.to_vec(), unhex("00800000011201000012345678").unwrap());
        assert_eq!(s.encrypted, unhex("b5e5bfdacbaf6cb7fb6bff871f").unwrap());
        assert_eq!(s.net_mic, unhex("035444ce83a670df").unwrap());
        assert_eq!(s.privacy_plaintext.to_vec(), unhex("000000000012345678b5e5bfdacbaf6c").unwrap());
        assert_eq!(s.pecb.to_vec(), unhex("6ca487507564").unwrap());
        assert_eq!(s.pdu, unhex("68eca487516765b5e5bfdacbaf6cb7fb6bff871f035444ce83a670df").unwrap());
        assert_eq!(decode_network(&s.pdu, &keys, 0x1234_5678).unwrap(), pdu);
    }

    #[test]
    fn message_18_access_pdu_matches_spec() {
        let keys = k2(&key(NET_KEY), &[0]);
        let a = encode_access(&unhex("0400000000").unwrap(), &key(APP_KEY), 7, 0x1201, 0xffff, 0x1234_5678);
        assert_eq!(a.nonce.to_vec(), unhex("01000000071201ffff12345678").unwrap());
        assert_eq!(a.encrypted, unhex("5a8bde6d91").unwrap());
        assert_eq!(a.trans_mic, unhex("06ea078a").unwrap());
        assert_eq!(a.lower_transport_pdu, unhex("665a8bde6d9106ea078a").unwrap());
        let pdu = NetworkPdu { ctl: false, ttl: 3, seq: 7, src: 0x1201, dst: 0xffff, transport_pdu: a.lower_transport_pdu.clone() };
        let s = encode_network(&pdu, &keys, 0x1234_5678);
        assert_eq!(s.pdu, unhex("6848cba437860e5673728a627fb938535508e21a6baf57").unwrap());
        let back = decode_network(&s.pdu, &keys, 0x1234_5678).unwrap();
        assert_eq!(back, pdu);
        let access = decode_access(&back.transport_pdu, &key(APP_KEY), back.seq, back.src, back.dst, 0x1234_5678).unwrap();
        assert_eq!(access, unhex("0400000000").unwrap());
    }

    #[test]
    fn message_21_group_message_with_odd_iv_index_matches_spec() {
        let keys = k2(&key(NET_KEY), &[0]);
        let a = encode_access(&unhex("d50a0048656c6c6f").unwrap(), &key(APP_KEY), 0x07080a, 0x1234, 0xc105, 0x1234_5677);
        assert_eq!(a.lower_transport_pdu, unhex("664d92e9dfcf3ab85b6e8fcf03").unwrap());
        let pdu = NetworkPdu { ctl: false, ttl: 3, seq: 0x07080a, src: 0x1234, dst: 0xc105, transport_pdu: a.lower_transport_pdu };
        let s = encode_network(&pdu, &keys, 0x1234_5677);
        assert_eq!(s.pdu, unhex("e84e8fbe003f58a4d61157bb76352ea6307eebfe0f30b83500e9").unwrap());
    }

    #[test]
    fn decode_rejects_wrong_key_wrong_iv_and_tampering() {
        let keys = k2(&key(NET_KEY), &[0]);
        let good = unhex("6848cba437860e5673728a627fb938535508e21a6baf57").unwrap();
        assert!(matches!(decode_network(&good, &k2(&key(NET_KEY_81), &[0]), 0x1234_5678), Err(DecodeError::WrongNid { .. })));
        assert_eq!(decode_network(&good, &keys, 0x1234_567A), Err(DecodeError::BadNetMic));
        for i in 1..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0x10;
            assert_eq!(decode_network(&bad, &keys, 0x1234_5678), Err(DecodeError::BadNetMic), "byte {i}");
        }
    }

    fn line(n: usize, relay: bool) -> Vec<FloodNode> {
        (0..n).map(|i| FloodNode { x: i as f64 * 10.0, y: 0.0, relay }).collect()
    }

    #[test]
    fn flooding_needs_enough_ttl_for_the_hops() {
        let nodes = line(5, true); // 4 hops from node 0 to node 4
        let base = FloodParams { range: 12.0, ttl: 4, src: 0, dst: Some(4), use_cache: true, seed: 1 };
        let ok = flood(&nodes, &base);
        assert!(ok.delivered_ms.is_some());
        // Each node transmits at most once thanks to the cache.
        assert!(ok.transmissions.len() <= 5);
        // TTL 3 gives only 3 hops: source (3) -> relay (2) -> relay (1) -> stop.
        let short = flood(&nodes, &FloodParams { ttl: 3, ..base });
        assert!(short.delivered_ms.is_none());
        assert!(short.reached[3] && !short.reached[4]);
        // TTL 0 is a single hop.
        let single = flood(&nodes, &FloodParams { ttl: 0, ..base });
        assert_eq!(single.transmissions.len(), 1);
    }

    #[test]
    fn non_relay_nodes_break_the_chain_and_no_cache_runs_away() {
        let mut nodes = line(5, true);
        nodes[2].relay = false;
        let base = FloodParams { range: 12.0, ttl: 10, src: 0, dst: Some(4), use_cache: true, seed: 1 };
        assert!(flood(&nodes, &base).delivered_ms.is_none());
        let storm = flood(&line(6, true), &FloodParams { use_cache: false, ttl: 20, ..base });
        assert!(storm.transmissions.len() > 50, "{}", storm.transmissions.len());
    }
}
