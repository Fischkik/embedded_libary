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
        0xFFF9 => "group: all IPT border routers",
        0xFFFA => "group: all IPT nodes",
        0xFFFB => "group: all directed forwarding nodes",
        0xFF00..=0xFFF8 => "group: fixed, reserved for future use",
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

// ---------------------------------------------------------------------------------
// Publish and subscribe (3.4.2)
// ---------------------------------------------------------------------------------

/// Does a node with this unicast address and these subscriptions accept a message sent
/// to `dst`? A message is for a node if it is sent to one of its own addresses, to a
/// group or virtual address it subscribes to, or to all-nodes.
pub fn accepts(own: u16, subscriptions: &[u16], dst: u16) -> bool {
    dst == own || dst == 0xFFFF || subscriptions.contains(&dst)
}

// ---------------------------------------------------------------------------------
// Segmentation (3.5.2.2, 3.5.3)
// ---------------------------------------------------------------------------------

/// An Upper Transport Access PDU of up to 15 octets fits into one unsegmented message;
/// larger ones are cut into segments of 12 octets (the last may be shorter).
pub const MAX_UNSEGMENTED: usize = 15;
pub const SEGMENT_OCTETS: usize = 12;
/// SegO and SegN are 5 bits: at most 32 segments.
pub const MAX_SEGMENTS: usize = 32;

/// One Segmented Access message: header and segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// SEG (1), AKF (1), AID (6) | SZMIC (1), SeqZero (13), SegO (5), SegN (5).
    pub header: [u8; 4],
    pub seg_o: u8,
    pub seg_n: u8,
    pub payload: Vec<u8>,
}

/// Cut an Upper Transport Access PDU into segments. `seq_zero` is the low 13 bits of
/// the sequence number of the first segment. Returns `None` if the PDU is too long.
pub fn segment(upper: &[u8], akf: bool, aid: u8, szmic: bool, seq: u32) -> Option<Vec<Segment>> {
    let n = upper.len().div_ceil(SEGMENT_OCTETS).max(1);
    if n > MAX_SEGMENTS {
        return None;
    }
    let seq_zero = seq & 0x1FFF;
    let seg_n = (n - 1) as u32;
    Some(
        upper
            .chunks(SEGMENT_OCTETS)
            .enumerate()
            .map(|(o, chunk)| {
                let o = o as u32;
                let first = 0x80 | (akf as u8) << 6 | (aid & 0x3F);
                let rest = (szmic as u32) << 23 | seq_zero << 10 | o << 5 | seg_n;
                let r = rest.to_be_bytes();
                Segment { header: [first, r[1], r[2], r[3]], seg_o: o as u8, seg_n: seg_n as u8, payload: chunk.to_vec() }
            })
            .collect(),
    )
}

/// Read SeqZero, SegO and SegN back from a segment header.
pub fn parse_segment_header(h: &[u8; 4]) -> (u16, u8, u8) {
    let rest = u32::from_be_bytes([0, h[1], h[2], h[3]]);
    (((rest >> 10) & 0x1FFF) as u16, ((rest >> 5) & 0x1F) as u8, (rest & 0x1F) as u8)
}

/// A Segment Acknowledgment message: SEG = 0, opcode 0x00, OBO, SeqZero, two RFU bits,
/// then AckedSegments with bit n set for every segment n received.
pub fn segment_ack(seq_zero: u16, received: &[bool]) -> [u8; 7] {
    let acked: u32 = received.iter().enumerate().filter(|(_, r)| **r).fold(0, |a, (i, _)| a | 1 << i);
    let mid = (seq_zero & 0x1FFF) << 2;
    let mut out = [0u8; 7];
    out[0] = 0x00;
    out[1..3].copy_from_slice(&mid.to_be_bytes());
    out[3..].copy_from_slice(&acked.to_be_bytes());
    out
}

/// One round of sending: which segments get lost on the way (`lost[i]`), what the
/// receiver acknowledges, and which segments are sent again in the next round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SarRound {
    pub sent: Vec<usize>,
    pub received_after: Vec<bool>,
    pub ack: [u8; 7],
}

/// Send segments in rounds: every round sends all segments not yet acknowledged; a
/// segment in `loss_plan[round]` is lost in that round.
pub fn sar_rounds(n: usize, seq_zero: u16, loss_plan: &[Vec<usize>], max_rounds: usize) -> Vec<SarRound> {
    let mut received = vec![false; n];
    let mut rounds = Vec::new();
    for r in 0..max_rounds {
        let sent: Vec<usize> = (0..n).filter(|i| !received[*i]).collect();
        if sent.is_empty() {
            break;
        }
        for &i in &sent {
            if !loss_plan.get(r).is_some_and(|l| l.contains(&i)) {
                received[i] = true;
            }
        }
        rounds.push(SarRound { sent, received_after: received.clone(), ack: segment_ack(seq_zero, &received) });
    }
    rounds
}

// ---------------------------------------------------------------------------------
// Friendship: a Low Power node polls a Friend that stores its messages (3.5.5, 3.6.5)
// ---------------------------------------------------------------------------------

/// Allowed ranges (Tables 3.37, 3.38, 3.41): ReceiveDelay 10..=255 ms, ReceiveWindow
/// 1..=255 ms, PollTimeout 1 s ..= 0x34BBFF x 100 ms (about 96 hours).
pub const RECEIVE_DELAY_MS: (f64, f64) = (10.0, 255.0);
pub const RECEIVE_WINDOW_MS: (f64, f64) = (1.0, 255.0);
pub const POLL_TIMEOUT_S: (f64, f64) = (1.0, 0x34BBFF as f64 * 0.1);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Friendship {
    /// How often the Low Power node wakes up and sends a Friend Poll.
    pub poll_interval_s: f64,
    /// Time from the end of the poll until the friend answers.
    pub receive_delay_ms: f64,
    /// How long the Low Power node listens for the answer.
    pub receive_window_ms: f64,
    /// Messages for the node arriving at the friend, per hour.
    pub messages_per_hour: f64,
    pub tx_a: f64,
    pub rx_a: f64,
    pub sleep_a: f64,
    /// Radio-on time to send one Friend Poll, including start-up.
    pub poll_tx_s: f64,
}

impl Friendship {
    /// Charge of one poll: send the poll, sleep through the delay, listen for the window.
    pub fn poll_charge_c(&self) -> f64 {
        self.poll_tx_s * self.tx_a + self.receive_window_ms * 1e-3 * self.rx_a
    }

    pub fn average_a(&self) -> f64 {
        self.poll_charge_c() / self.poll_interval_s + self.sleep_a
    }

    /// A node that never sleeps but listens all the time, as a relay does.
    pub fn always_listening_a(&self) -> f64 {
        self.rx_a
    }

    /// A message for the node waits at the friend for the next poll: half an interval on
    /// average, a whole one at worst, plus the receive delay.
    pub fn average_delay_s(&self) -> f64 {
        self.poll_interval_s / 2.0 + self.receive_delay_ms * 1e-3
    }

    pub fn battery_life_s(&self, capacity_mah: f64) -> f64 {
        capacity_mah * 3.6 / self.average_a()
    }
}

// ---------------------------------------------------------------------------------
// Provisioning (section 5), AES-CMAC algorithm. Formulas as in Zephyr's
// subsys/bluetooth/mesh/crypto.c: ConfirmationSalt = s1(ConfirmationInputs);
// ConfirmationKey = k1(ECDHSecret, ConfirmationSalt, "prck"); Confirmation =
// AES-CMAC(ConfirmationKey, Random || AuthValue); ProvisioningSalt =
// s1(ConfirmationSalt || RandomProvisioner || RandomDevice); SessionKey, SessionNonce
// (bytes 3..16) and DeviceKey = k1(ECDHSecret, ProvisioningSalt, "prsk" / "prsn" /
// "prdk"); the 25-octet provisioning data is encrypted with AES-CCM and an 8-octet MIC.
// The elliptic-curve step that produces ECDHSecret is not computed here.
// ---------------------------------------------------------------------------------

/// One side's view of a provisioning run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvSide {
    pub confirmation_salt: Key,
    pub confirmation_key: Key,
    pub own_confirmation: Key,
}

pub fn prov_side(ecdh_secret: &[u8; 32], conf_inputs: &[u8], own_random: &Key, auth: &Key) -> ProvSide {
    let confirmation_salt = s1(conf_inputs);
    let confirmation_key = k1(ecdh_secret, &confirmation_salt, b"prck");
    let own_confirmation = cmac(&confirmation_key, &[own_random.as_slice(), auth.as_slice()].concat());
    ProvSide { confirmation_salt, confirmation_key, own_confirmation }
}

/// Does a revealed random number match the confirmation received earlier?
pub fn check_confirmation(side: &ProvSide, received_confirmation: &Key, revealed_random: &Key, auth: &Key) -> bool {
    cmac(&side.confirmation_key, &[revealed_random.as_slice(), auth.as_slice()].concat()) == *received_confirmation
}

/// The provisioning data: NetKey (16), key index (2), flags (1), IV index (4), unicast address (2).
pub fn provisioning_data(net_key: &Key, key_index: u16, flags: u8, iv_index: u32, address: u16) -> Vec<u8> {
    let mut d = net_key.to_vec();
    d.extend(key_index.to_be_bytes());
    d.push(flags);
    d.extend(iv_index.to_be_bytes());
    d.extend(address.to_be_bytes());
    d
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionKeys {
    pub provisioning_salt: Key,
    pub session_key: Key,
    pub session_nonce: [u8; 13],
    pub device_key: Key,
}

pub fn session_keys(ecdh_secret: &[u8; 32], confirmation_salt: &Key, random_provisioner: &Key, random_device: &Key) -> SessionKeys {
    let provisioning_salt = s1(&[confirmation_salt.as_slice(), random_provisioner.as_slice(), random_device.as_slice()].concat());
    let nonce_full = k1(ecdh_secret, &provisioning_salt, b"prsn");
    SessionKeys {
        provisioning_salt,
        session_key: k1(ecdh_secret, &provisioning_salt, b"prsk"),
        session_nonce: nonce_full[3..].try_into().unwrap_or([0; 13]),
        device_key: k1(ecdh_secret, &provisioning_salt, b"prdk"),
    }
}

/// AuthValue for Output OOB with a number shown on the device: the number in the last
/// bytes, zeros before. No OOB: all zeros.
pub fn auth_value(number: Option<u32>) -> Key {
    let mut a = [0u8; 16];
    if let Some(n) = number {
        a[12..].copy_from_slice(&n.to_be_bytes());
    }
    a
}

/// A man in the middle: the attacker has done the key exchange with each side separately
/// (`secret_p` with the provisioner) and must present a confirmation to the provisioner
/// before the provisioner reveals its random number. It uses `guessed_auth`. Returns
/// whether the provisioner accepts the attacker's revealed random number.
pub fn mitm_accepted(secret_p: &[u8; 32], conf_inputs: &[u8], true_auth: &Key, guessed_auth: &Key, attacker_random: &Key, provisioner_random: &Key) -> bool {
    let provisioner = prov_side(secret_p, conf_inputs, provisioner_random, true_auth);
    let fake = prov_side(secret_p, conf_inputs, attacker_random, guessed_auth);
    check_confirmation(&provisioner, &fake.own_confirmation, attacker_random, true_auth)
}

// ---- Replay protection and the IV Index (3.9.3, 3.9.8, 3.11.5) ----

/// Number of 24-bit sequence numbers: 3.9.3, "an element can transmit 16,777,216 messages
/// before repeating a nonce".
pub const SEQ_COUNT: u32 = 1 << 24;
/// 3.11.5: "A node shall not start an IV Update procedure more often than once every 192
/// hours." Each of the two states lasts at least 96 hours.
pub const IV_UPDATE_MIN_PERIOD_H: f64 = 192.0;
pub const IV_STATE_MIN_H: f64 = 96.0;
/// 3.11.5: a beacon with an IV Index "greater than the last known IV Index + 42" is ignored.
pub const IV_RECOVERY_LIMIT: u32 = 42;

/// IVISeq (3.9.8): "the IV Index is the four most significant octets and the sequence
/// number is the three least significant octets".
pub fn iviseq(iv_index: u32, seq: u32) -> u64 {
    (iv_index as u64) << 24 | (seq & 0xFF_FFFF) as u64
}

/// The replay protection list (3.9.8): per source address, the last valid IVISeq. "Any
/// future messages from the same originating element that contain an IVISeq value that is
/// lower than or equal to the last valid IVISeq value ... shall be discarded."
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayList {
    pub entries: std::collections::BTreeMap<u16, u64>,
}

impl ReplayList {
    /// `Ok(())` and the entry updated if the message is new, `Err(last)` if it is a replay.
    pub fn check(&mut self, src: u16, iv_index: u32, seq: u32) -> Result<(), u64> {
        let v = iviseq(iv_index, seq);
        match self.entries.get(&src) {
            Some(&last) if v <= last => Err(last),
            _ => {
                self.entries.insert(src, v);
                Ok(())
            }
        }
    }
}

/// The IV Index a receiver uses for a packet whose IVI bit is `ivi` (3.11.5, Table 3.84): a
/// node accepts the current IV Index and the one before it, in both states, and the IVI bit
/// (the least significant bit) says which of the two the sender used.
pub fn rx_iv_index(current: u32, ivi: bool) -> u32 {
    if (current & 1 == 1) == ivi { current } else { current.wrapping_sub(1) }
}

/// The IV Index used for sending (Table 3.84): the current one in Normal Operation, the one
/// before it while the IV Update is in progress.
pub fn tx_iv_index(current: u32, update_in_progress: bool) -> u32 {
    if update_in_progress { current.wrapping_sub(1) } else { current }
}

/// Whether a beacon announcing `beacon_iv` is accepted in Normal Operation (3.11.5).
pub fn beacon_iv_accepted(last_known: u32, beacon_iv: u32) -> bool {
    beacon_iv >= last_known && beacon_iv - last_known <= IV_RECOVERY_LIMIT
}

/// How long the sequence numbers last when one message is sent every `interval_s`.
pub fn seq_lifetime_s(interval_s: f64) -> f64 {
    SEQ_COUNT as f64 * interval_s
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

    #[test]
    fn address_kinds_follow_the_specification_table() {
        assert_eq!(address_kind(0x0000), "unassigned");
        assert!(address_kind(0x0001).starts_with("unicast") && address_kind(0x7FFF).starts_with("unicast"));
        assert!(address_kind(0x8000).starts_with("virtual") && address_kind(0xBFFF).starts_with("virtual"));
        assert_eq!(address_kind(0xC000), "group");
        assert_eq!(address_kind(0xFEFF), "group");
        assert_eq!(address_kind(0xFFFB), "group: all directed forwarding nodes");
        assert_eq!(address_kind(0xFFFF), "group: all nodes");
        assert!(accepts(0x0005, &[0xC001], 0xC001) && accepts(0x0005, &[], 0x0005) && accepts(0x0005, &[], 0xFFFF));
        assert!(!accepts(0x0005, &[0xC001], 0xC002));
    }

    #[test]
    fn segmentation_and_acknowledgment() {
        // 30 octets: 12 + 12 + 6, SegN = 2.
        let upper: Vec<u8> = (0..30).collect();
        let segs = segment(&upper, true, 0x26, false, 0x3129AB).unwrap();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs.iter().map(|s| s.payload.len()).collect::<Vec<_>>(), [12, 12, 6]);
        assert_eq!(segs[0].header[0], 0x80 | 0x40 | 0x26);
        for (o, s) in segs.iter().enumerate() {
            assert_eq!(parse_segment_header(&s.header), ((0x3129AB & 0x1FFF) as u16, o as u8, 2));
        }
        assert!(segment(&vec![0; 12 * 32], false, 0, false, 0).is_some());
        assert!(segment(&vec![0; 12 * 32 + 1], false, 0, false, 0).is_none());
        // Segments 0 and 2 arrive: AckedSegments = 0b101.
        let ack = segment_ack(0x09AB, &[true, false, true]);
        assert_eq!(&ack[3..], &[0, 0, 0, 0b101]);
        assert_eq!(u16::from_be_bytes([ack[1], ack[2]]) >> 2, 0x09AB);
        // Round 1 loses segment 1; round 2 sends only segment 1.
        let rounds = sar_rounds(3, 0x09AB, &[vec![1]], 5);
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[1].sent, [1]);
        assert!(rounds[1].received_after.iter().all(|r| *r));
    }

    #[test]
    fn message_6_segmented_access_matches_spec() {
        // Mesh Protocol 1.1 sample data 8.3.6 (Message #6), as in BlueZ unit/test-mesh-crypto.c:
        // device key, SEQ 0x3129AB, an access payload of 20 octets with a 32-bit TransMIC.
        let dev_key: Key = unhex("9d6dd0e96eb25dc19a40ed9914f8f03f").unwrap().try_into().unwrap();
        let nonce: [u8; 13] = unhex("02003129ab0003120112345678").unwrap().try_into().unwrap();
        let access = unhex("0056341263964771734fbd76e3b40519d1d94a48").unwrap();
        let upper = ccm_encrypt(&dev_key, &nonce, &[], &access, 4);
        assert_eq!(upper, unhex("ee9dddfd2169326d23f3afdfcfdc18c52fdef772e0e17308").unwrap());
        let segs = segment(&upper, false, 0, false, 0x3129AB).unwrap();
        let lower: Vec<Vec<u8>> = segs.iter().map(|s| [s.header.to_vec(), s.payload.clone()].concat()).collect();
        assert_eq!(lower, [unhex("8026ac01ee9dddfd2169326d23f3afdf").unwrap(), unhex("8026ac21cfdc18c52fdef772e0e17308").unwrap()]);
    }

    #[test]
    fn friendship_power_and_delay() {
        let f = Friendship { poll_interval_s: 10.0, receive_delay_ms: 100.0, receive_window_ms: 100.0, messages_per_hour: 6.0, tx_a: 7.1e-3, rx_a: 6.5e-3, sleep_a: 1.6e-6, poll_tx_s: 0.5e-3 };
        // 0.5 ms x 7.1 mA + 100 ms x 6.5 mA = 653.55 µC per poll.
        assert!((f.poll_charge_c() - 653.55e-6).abs() < 1e-9);
        // Every 10 s: 65.4 µA + 1.6 µA = 66.96 µA - a hundred times less than listening all the time.
        assert!((f.average_a() - 66.955e-6).abs() < 1e-9, "{}", f.average_a());
        assert!(f.always_listening_a() / f.average_a() > 95.0);
        assert!((f.average_delay_s() - 5.1).abs() < 1e-12);
        // A 10 ms window cuts the cost of each poll almost tenfold.
        assert!(Friendship { receive_window_ms: 10.0, ..f }.average_a() < 0.15 * f.average_a());
        assert_eq!(POLL_TIMEOUT_S.1, 345_599.9);
    }

    #[test]
    fn provisioning_confirmation_session_and_mitm() {
        // k1 and s1 are checked against the spec's sample data above; this test checks
        // that the provisioning steps fit together, not their exact bytes.
        let secret = [0x5Au8; 32];
        let inputs = vec![0x11u8; 145];
        let (rp, rd) = ([0x01u8; 16], [0x02u8; 16]);
        let auth = auth_value(Some(123_456));
        let p = prov_side(&secret, &inputs, &rp, &auth);
        let d = prov_side(&secret, &inputs, &rd, &auth);
        assert_eq!(p.confirmation_key, d.confirmation_key);
        assert!(check_confirmation(&p, &d.own_confirmation, &rd, &auth));
        assert!(check_confirmation(&d, &p.own_confirmation, &rp, &auth));
        // A wrong random number or AuthValue fails.
        assert!(!check_confirmation(&p, &d.own_confirmation, &rp, &auth));
        assert!(!check_confirmation(&p, &d.own_confirmation, &rd, &auth_value(Some(123_457))));
        // Both sides derive the same session key and nonce; the data round-trips.
        let k = session_keys(&secret, &p.confirmation_salt, &rp, &rd);
        let data = provisioning_data(&[0x7D; 16], 0x0000, 0x00, 0x1234_5678, 0x0B0C);
        assert_eq!(data.len(), 25);
        let enc = ccm_encrypt(&k.session_key, &k.session_nonce, &[], &data, 8);
        assert_eq!(enc.len(), 33);
        assert_eq!(ccm_decrypt(&k.session_key, &k.session_nonce, &[], &enc, 8), Some(data));
        // Man in the middle: with no OOB (AuthValue 0) the attacker's confirmation is
        // accepted; with a number shown on the device, a guess fails.
        let sp = [0x33u8; 32];
        assert!(mitm_accepted(&sp, &inputs, &auth_value(None), &auth_value(None), &[9; 16], &rp));
        assert!(!mitm_accepted(&sp, &inputs, &auth, &auth_value(Some(0)), &[9; 16], &rp));
        assert!(mitm_accepted(&sp, &inputs, &auth, &auth, &[9; 16], &rp), "a correct guess would work");
    }
    #[test]
    // The digit groups mirror the layout: 32-bit IV index, then 24-bit sequence number.
    #[allow(clippy::unusual_byte_groupings)]
    fn replay_list_and_iv_index_follow_the_spec() {
        // IVISeq layout.
        assert_eq!(iviseq(0x1234_5678, 0x00_0007), 0x1234_5678_00_0007);
        // A lower or equal IVISeq is discarded; a higher one replaces the entry.
        let mut rpl = ReplayList::default();
        assert_eq!(rpl.check(0x1201, 0x1234_5678, 7), Ok(()));
        assert_eq!(rpl.check(0x1201, 0x1234_5678, 7), Err(0x1234_5678_00_0007));
        assert!(rpl.check(0x1201, 0x1234_5678, 3).is_err());
        assert_eq!(rpl.check(0x1234, 0x1234_5678, 3), Ok(()), "another source has its own entry");
        assert_eq!(rpl.check(0x1201, 0x1234_5678, 8), Ok(()));
        // After an IV Update, SEQ 0 under the new IV Index beats any SEQ under the old one.
        assert_eq!(rpl.check(0x1201, 0x1234_5679, 0), Ok(()));
        assert!(rpl.check(0x1201, 0x1234_5678, 0xFF_FFFF).is_err());

        // The examples of 3.11.5: current IV Index 0x00101847 in Normal Operation.
        assert_eq!(tx_iv_index(0x0010_1847, false), 0x0010_1847);
        assert_eq!(rx_iv_index(0x0010_1847, true), 0x0010_1847);
        assert_eq!(rx_iv_index(0x0010_1847, false), 0x0010_1846);
        // IV Update in Progress: current 0x00101848, transmit with 0x00101847.
        assert_eq!(tx_iv_index(0x0010_1848, true), 0x0010_1847);
        assert_eq!(rx_iv_index(0x0010_1848, true), 0x0010_1847);
        assert_eq!(rx_iv_index(0x0010_1848, false), 0x0010_1848);

        // Beacons: at most 42 ahead; 42 updates of 192 h each are the spec's 48 weeks.
        assert!(beacon_iv_accepted(100, 142) && !beacon_iv_accepted(100, 143) && !beacon_iv_accepted(100, 99));
        assert_eq!(IV_RECOVERY_LIMIT as f64 * IV_UPDATE_MIN_PERIOD_H / (24.0 * 7.0), 48.0);

        // 3.9.3: one message every five seconds lasts 2.6 years; one per second 194 days.
        assert!((seq_lifetime_s(5.0) / (365.25 * 86_400.0) - 2.66).abs() < 0.01);
        assert!((seq_lifetime_s(1.0) / 86_400.0 - 194.18).abs() < 0.01);
    }

    #[test]
    fn replayed_network_pdu_decodes_but_is_refused() {
        // A recorded packet still decrypts perfectly; only the replay list stops it. With an
        // IV Index two steps on, the IVI bit picks the wrong IV Index and decryption fails.
        let keys = k2(&key(NET_KEY), &[0]);
        let pdu = NetworkPdu { ctl: false, ttl: 3, seq: 7, src: 0x1201, dst: 0xffff, transport_pdu: unhex("665a8bde6d9106ea078a").unwrap() };
        let bytes = encode_network(&pdu, &keys, 0x1234_5678).pdu;
        let mut rpl = ReplayList::default();
        for (i, expect_ok) in [true, false].into_iter().enumerate() {
            let iv = rx_iv_index(0x1234_5678, bytes[0] & 0x80 != 0);
            let got = decode_network(&bytes, &keys, iv).unwrap();
            assert_eq!(rpl.check(got.src, iv, got.seq).is_ok(), expect_ok, "reception {i}");
        }
        let iv = rx_iv_index(0x1234_567A, bytes[0] & 0x80 != 0);
        assert_eq!(iv, 0x1234_567A);
        assert_eq!(decode_network(&bytes, &keys, iv), Err(DecodeError::BadNetMic));
    }

    #[test]
    fn provisioning_matches_spec_sample_data_8_7() {
        // Mesh Protocol 1.1 sections 8.7.8 - 8.7.12 (PB-ADV provisioning, no OOB). Values
        // copied by script from the specification text, not typed.
        let ecdh: [u8; 32] = unhex("ab85843a2f6d883f62e5684b38e307335fe6e1945ecd19604105c6f23221eb69").unwrap().try_into().unwrap();
        let inputs = unhex("00010001000000000000000000000000002c31a47b5779809ef44cb5eaaf5c3e43d5f8faad4a8794cb987e9b03745c78dd919512183898dfbecd52e2408e43871fd021109117bd3ed4eaf8437743715d4ff465e43ff23d3f1b9dc7dfc04da8758184dbc966204796eccf0d6cf5e16500cc0201d048bcbbd899eeefc424164e33c201c2b010ca6b4d43a8a155cad8ecb279").unwrap();
        let (rp, rd) = (key("8b19ac31d58b124c946209b5db1021b9"), key("55a2a2bca04cd32ff6f346bd0a0c1a3a"));
        let auth = auth_value(None);
        assert_eq!(auth, key("00000000000000000000000000000000"));
        let p = prov_side(&ecdh, &inputs, &rp, &auth);
        assert_eq!(p.confirmation_salt, key("5faabe187337c71cc6c973369dcaa79a"));
        assert_eq!(p.confirmation_key, key("e31fe046c68ec339c425fc6629f0336f"));
        assert_eq!(p.own_confirmation, key("b38a114dfdca1fe153bd2c1e0dc46ac2"));
        let d = prov_side(&ecdh, &inputs, &rd, &auth);
        assert_eq!(d.own_confirmation, key("eeba521c196b52cc2e37aa40329f554e"));
        assert!(check_confirmation(&p, &d.own_confirmation, &rd, &auth));
        let k = session_keys(&ecdh, &p.confirmation_salt, &rp, &rd);
        assert_eq!(k.provisioning_salt, key("a21c7d45f201cf9489a2fb57145015b4"));
        assert_eq!(k.session_key, key("c80253af86b33dfa450bbdb2a191fea3"));
        assert_eq!(k.session_nonce.to_vec(), unhex("da7ddbe78b5f62b81d6847487e").unwrap());
        assert_eq!(k.device_key, key("0520adad5e0142aa3e325087b4ec16d8"));
        let data = provisioning_data(&key("efb2255e6422d330088e09bb015ed707"), 0x0567, 0x00, 0x0102_0304, 0x0b0c);
        assert_eq!(data, unhex("efb2255e6422d330088e09bb015ed707056700010203040b0c").unwrap());
        let enc = ccm_encrypt(&k.session_key, &k.session_nonce, &[], &data, 8);
        assert_eq!(enc, unhex("d0bd7f4a89a2ff6222af59a90a60ad58acfe3123356f5cec2973e0ec50783b10c7").unwrap());
    }

}
