//! Bluetooth Low Energy link layer, bit-exact.
//!
//! References are to the Bluetooth Core Specification 5.4, Volume 6 (Low Energy
//! Controller): Part A is the physical layer, Part B the link layer, Part C sample data.
//! The tests at the bottom check this code against the sample data in Part C.

use crate::rng::Rng;

/// Access address used by all advertising packets (Vol 6 Part B 2.1.2).
pub const ADV_ACCESS_ADDRESS: u32 = 0x8E89_BED6;
/// CRC start value for advertising packets (Vol 6 Part B 3.1.1).
pub const ADV_CRC_INIT: u32 = 0x55_5555;
/// Time between a packet and the reply to it, in microseconds (Vol 6 Part B 4.1.1).
pub const T_IFS_US: f64 = 150.0;

/// Centre frequency in MHz of a channel index 0..=39 (Vol 6 Part B, Table 1.3).
/// 37, 38 and 39 are the advertising channels; they sit at the bottom, in a gap, and at
/// the top of the band so that they dodge the usual Wi-Fi channels.
pub fn channel_mhz(index: u8) -> u32 {
    match index {
        37 => 2402,
        38 => 2426,
        39 => 2480,
        0..=10 => 2404 + 2 * index as u32,
        _ => 2428 + 2 * (index.min(36) as u32 - 11),
    }
}

/// 24-bit CRC over the PDU (Vol 6 Part B 3.1.1).
///
/// Returns the three CRC bytes in the order they go on air (each byte is then sent
/// least significant bit first, like every other byte).
pub fn crc24(pdu: &[u8], init: u32) -> [u8; 3] {
    let mut lfsr = init & 0xFF_FFFF;
    for &byte in pdu {
        for i in 0..8 {
            let feedback = ((lfsr >> 23) & 1) ^ ((byte >> i) as u32 & 1);
            lfsr = (lfsr << 1) & 0xFF_FFFF;
            if feedback == 1 {
                // x^24 + x^10 + x^9 + x^6 + x^4 + x^3 + x + 1
                lfsr ^= 0x00_065B;
            }
        }
    }
    // The CRC is sent from position 23 down to 0, i.e. bit-reversed per byte.
    [((lfsr >> 16) as u8).reverse_bits(), ((lfsr >> 8) as u8).reverse_bits(), (lfsr as u8).reverse_bits()]
}

/// The whitening bit sequence for a channel (Vol 6 Part B 3.2): a 7-bit shift register
/// with polynomial x^7 + x^4 + 1, started from the channel index.
pub fn whitening_bits(channel: u8, count: usize) -> Vec<bool> {
    let mut p = [false; 7];
    p[0] = true;
    for i in 0..6 {
        p[1 + i] = (channel >> (5 - i)) & 1 == 1;
    }
    (0..count)
        .map(|_| {
            let out = p[6];
            let p3 = p[3];
            for i in (1..7).rev() {
                p[i] = p[i - 1];
            }
            p[0] = out;
            p[4] = p3 ^ out;
            out
        })
        .collect()
}

/// Whiten or de-whiten in place (the operation is its own inverse).
pub fn whiten(data: &mut [u8], channel: u8) {
    let w = whitening_bits(channel, data.len() * 8);
    for (i, byte) in data.iter_mut().enumerate() {
        for bit in 0..8 {
            if w[i * 8 + bit] {
                *byte ^= 1 << bit;
            }
        }
    }
}

/// Legacy advertising PDU types (Vol 6 Part B, Table 2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PduType {
    AdvInd,
    AdvDirectInd,
    AdvNonconnInd,
    ScanReq,
    ScanRsp,
    ConnectInd,
    AdvScanInd,
    Other(u8),
}

impl PduType {
    pub const COMMON: [PduType; 4] = [PduType::AdvInd, PduType::AdvNonconnInd, PduType::AdvScanInd, PduType::ScanRsp];

    pub fn from_bits(v: u8) -> Self {
        match v & 0x0F {
            0 => Self::AdvInd,
            1 => Self::AdvDirectInd,
            2 => Self::AdvNonconnInd,
            3 => Self::ScanReq,
            4 => Self::ScanRsp,
            5 => Self::ConnectInd,
            6 => Self::AdvScanInd,
            o => Self::Other(o),
        }
    }

    pub fn bits(self) -> u8 {
        match self {
            Self::AdvInd => 0,
            Self::AdvDirectInd => 1,
            Self::AdvNonconnInd => 2,
            Self::ScanReq => 3,
            Self::ScanRsp => 4,
            Self::ConnectInd => 5,
            Self::AdvScanInd => 6,
            Self::Other(o) => o & 0x0F,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::AdvInd => "ADV_IND",
            Self::AdvDirectInd => "ADV_DIRECT_IND",
            Self::AdvNonconnInd => "ADV_NONCONN_IND",
            Self::ScanReq => "SCAN_REQ",
            Self::ScanRsp => "SCAN_RSP",
            Self::ConnectInd => "CONNECT_IND",
            Self::AdvScanInd => "ADV_SCAN_IND",
            Self::Other(_) => "other / extended",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Self::AdvInd => "\"I am here, and you may connect to me or ask for more.\"",
            Self::AdvDirectInd => "\"I am here - and I want one specific device to connect.\"",
            Self::AdvNonconnInd => "\"I am here.\" Broadcast only: no connections, no questions. Beacons and Bluetooth Mesh use this.",
            Self::ScanReq => "A scanner asking an advertiser for more data.",
            Self::ScanRsp => "The advertiser's answer to a scan request.",
            Self::ConnectInd => "\"Let's connect\" - carries all the connection timing.",
            Self::AdvScanInd => "\"I am here, you may ask for more, but you cannot connect.\"",
            Self::Other(_) => "A type this demo does not decode.",
        }
    }
}

/// One advertising-data structure: length, type, data (Core Vol 3 Part C 11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdStructure {
    pub ad_type: u8,
    pub data: Vec<u8>,
}

impl AdStructure {
    pub fn flags(v: u8) -> Self {
        Self { ad_type: 0x01, data: vec![v] }
    }
    pub fn complete_name(name: &str) -> Self {
        Self { ad_type: 0x09, data: name.as_bytes().to_vec() }
    }
    pub fn manufacturer(company: u16, data: &[u8]) -> Self {
        let mut d = company.to_le_bytes().to_vec();
        d.extend_from_slice(data);
        Self { ad_type: 0xFF, data: d }
    }
    pub fn mesh_message(network_pdu: &[u8]) -> Self {
        Self { ad_type: 0x2A, data: network_pdu.to_vec() }
    }
}

/// Human name of an AD type (Bluetooth Assigned Numbers 2.3).
pub fn ad_type_name(t: u8) -> &'static str {
    match t {
        0x01 => "Flags",
        0x02 => "Incomplete list of 16-bit service UUIDs",
        0x03 => "Complete list of 16-bit service UUIDs",
        0x08 => "Shortened local name",
        0x09 => "Complete local name",
        0x0A => "TX power level",
        0x16 => "Service data (16-bit UUID)",
        0x29 => "PB-ADV (mesh provisioning)",
        0x2A => "Mesh message",
        0x2B => "Mesh beacon",
        0xFF => "Manufacturer specific data",
        _ => "Unknown type",
    }
}

pub fn build_ad(structures: &[AdStructure]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in structures {
        out.push(s.data.len() as u8 + 1);
        out.push(s.ad_type);
        out.extend_from_slice(&s.data);
    }
    out
}

/// Parse advertising data. Returns what could be parsed plus an error for the rest.
pub fn parse_ad(payload: &[u8]) -> (Vec<AdStructure>, Option<String>) {
    let mut out = Vec::new();
    let mut i = 0;
    while i < payload.len() {
        let len = payload[i] as usize;
        if len == 0 {
            return (out, Some(format!("byte {i}: length 0 ends the data early")));
        }
        if i + 1 + len > payload.len() {
            return (out, Some(format!("byte {i}: length {len} runs past the end of the packet")));
        }
        out.push(AdStructure { ad_type: payload[i + 1], data: payload[i + 2..i + 1 + len].to_vec() });
        i += 1 + len;
    }
    (out, None)
}

/// A legacy advertising PDU: 2 header bytes, the advertiser's address, advertising data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvPdu {
    pub pdu_type: PduType,
    pub ch_sel: bool,
    /// true = the address is random, false = public.
    pub tx_add: bool,
    pub rx_add: bool,
    /// Address as written (most significant byte first), e.g. C1:A2:A3:A4:A5:A6.
    pub adv_a: [u8; 6],
    pub adv_data: Vec<u8>,
}

impl AdvPdu {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![
            self.pdu_type.bits() | (self.ch_sel as u8) << 5 | (self.tx_add as u8) << 6 | (self.rx_add as u8) << 7,
            (6 + self.adv_data.len()) as u8,
        ];
        out.extend(self.adv_a.iter().rev()); // addresses go least significant byte first
        out.extend_from_slice(&self.adv_data);
        out
    }

    pub fn from_bytes(pdu: &[u8]) -> Result<Self, String> {
        if pdu.len() < 8 {
            return Err(format!("only {} bytes: need 2 header bytes and a 6-byte address", pdu.len()));
        }
        let len = pdu[1] as usize;
        if len != pdu.len() - 2 {
            return Err(format!("header says {len} payload bytes, but {} follow", pdu.len() - 2));
        }
        let mut adv_a = [0u8; 6];
        for i in 0..6 {
            adv_a[i] = pdu[7 - i];
        }
        Ok(Self {
            pdu_type: PduType::from_bits(pdu[0]),
            ch_sel: pdu[0] & 0x20 != 0,
            tx_add: pdu[0] & 0x40 != 0,
            rx_add: pdu[0] & 0x80 != 0,
            adv_a,
            adv_data: pdu[8..].to_vec(),
        })
    }
}

/// A complete packet as it goes on air on the LE 1M PHY (Vol 6 Part B 2.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AirPacket {
    pub preamble: u8,
    pub access_address: [u8; 4],
    /// PDU before whitening.
    pub pdu: Vec<u8>,
    /// CRC before whitening.
    pub crc: [u8; 3],
    /// PDU + CRC after whitening.
    pub whitened: Vec<u8>,
}

impl AirPacket {
    /// All bytes in transmit order: preamble, access address, whitened PDU and CRC.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = vec![self.preamble];
        out.extend_from_slice(&self.access_address);
        out.extend_from_slice(&self.whitened);
        out
    }

    /// Duration on air in microseconds at 1 Mbit/s (1 bit = 1 microsecond).
    pub fn duration_us(&self) -> f64 {
        self.bytes().len() as f64 * 8.0
    }
}

/// Build the on-air packet for a PDU.
pub fn assemble(pdu: &[u8], channel: u8, access_address: u32, crc_init: u32) -> AirPacket {
    let crc = crc24(pdu, crc_init);
    let mut whitened = pdu.to_vec();
    whitened.extend_from_slice(&crc);
    whiten(&mut whitened, channel);
    AirPacket {
        // The preamble alternates and starts with the same bit as the access address.
        preamble: if access_address & 1 == 1 { 0x55 } else { 0xAA },
        access_address: access_address.to_le_bytes(),
        pdu: pdu.to_vec(),
        crc,
        whitened,
    }
}

/// What a receiver makes of the whitened part of a packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Received {
    pub pdu: Vec<u8>,
    pub crc_received: [u8; 3],
    pub crc_computed: [u8; 3],
}

impl Received {
    pub fn crc_ok(&self) -> bool {
        self.crc_received == self.crc_computed
    }
}

/// De-whiten and check the CRC, like a receiver tuned to `channel` would.
pub fn receive(whitened: &[u8], channel: u8, crc_init: u32) -> Option<Received> {
    if whitened.len() < 3 {
        return None;
    }
    let mut data = whitened.to_vec();
    whiten(&mut data, channel);
    let split = data.len() - 3;
    let pdu = data[..split].to_vec();
    let crc_computed = crc24(&pdu, crc_init);
    Some(Received { pdu, crc_received: [data[split], data[split + 1], data[split + 2]], crc_computed })
}

/// Channel Selection Algorithm #1 (Vol 6 Part B 4.5.8.2): which data channel a
/// connection uses for each connection event.
#[derive(Clone, Debug)]
pub struct Csa1 {
    pub hop_increment: u8,
    pub last_unmapped: u8,
    /// `used[i]` is true if data channel `i` (0..=36) may be used.
    pub used: [bool; 37],
}

/// The result of one hop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hop {
    pub unmapped: u8,
    pub channel: u8,
    pub remapped: bool,
}

impl Csa1 {
    pub fn new(hop_increment: u8, used: [bool; 37]) -> Self {
        Self { hop_increment, last_unmapped: 0, used }
    }

    pub fn next_hop(&mut self) -> Hop {
        let unmapped = (self.last_unmapped + self.hop_increment) % 37;
        self.last_unmapped = unmapped;
        if self.used[unmapped as usize] {
            return Hop { unmapped, channel: unmapped, remapped: false };
        }
        let table: Vec<u8> = (0..37u8).filter(|c| self.used[*c as usize]).collect();
        if table.is_empty() {
            return Hop { unmapped, channel: unmapped, remapped: false };
        }
        Hop { unmapped, channel: table[unmapped as usize % table.len()], remapped: true }
    }
}

/// One advertising packet in a simulated timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvTx {
    pub start_ms: f64,
    pub end_ms: f64,
    pub channel: u8,
    pub event: usize,
    pub heard: bool,
}

/// A stretch of time the scanner's receiver is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScanWindow {
    pub start_ms: f64,
    pub end_ms: f64,
    pub channel: u8,
}

/// Advertiser and scanner timing settings (Vol 6 Part B 4.4.2 and 4.4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvScanParams {
    pub adv_interval_ms: f64,
    /// Length of one advertising packet on air.
    pub packet_ms: f64,
    /// Gap between the packets on channels 37, 38 and 39 inside one event.
    pub channel_gap_ms: f64,
    pub scan_interval_ms: f64,
    pub scan_window_ms: f64,
    pub scan_offset_ms: f64,
    pub duration_ms: f64,
    pub seed: u64,
}

#[derive(Clone, Debug, Default)]
pub struct AdvScanTrace {
    pub packets: Vec<AdvTx>,
    pub windows: Vec<ScanWindow>,
    /// When the scanner first received a whole packet.
    pub first_heard_ms: Option<f64>,
}

/// Simulate an advertiser and a scanner that are not synchronised.
///
/// Each advertising event sends the same packet on 37, 38, 39. Events are spaced by
/// advInterval plus a random advDelay of 0..10 ms. The scanner listens on one
/// advertising channel per scan interval, for the length of the scan window.
pub fn simulate_adv_scan(p: &AdvScanParams) -> AdvScanTrace {
    let mut rng = Rng::new(p.seed);
    let mut trace = AdvScanTrace::default();
    let interval = p.scan_interval_ms.max(0.1);
    let window = p.scan_window_ms.clamp(0.0, interval);
    let mut t = p.scan_offset_ms.rem_euclid(interval) - interval;
    let mut k = 0usize;
    while t < p.duration_ms {
        trace.windows.push(ScanWindow { start_ms: t, end_ms: t + window, channel: 37 + (k % 3) as u8 });
        t += interval;
        k += 1;
    }
    let mut t = 0.0;
    let mut event = 0usize;
    while t < p.duration_ms {
        for (i, channel) in [37u8, 38, 39].into_iter().enumerate() {
            let start = t + i as f64 * (p.packet_ms + p.channel_gap_ms);
            let end = start + p.packet_ms;
            let heard = trace.windows.iter().any(|w| w.channel == channel && w.start_ms <= start && end <= w.end_ms);
            if heard && trace.first_heard_ms.is_none() {
                trace.first_heard_ms = Some(end);
            }
            trace.packets.push(AdvTx { start_ms: start, end_ms: end, channel, event, heard });
        }
        t += p.adv_interval_ms.max(1.0) + rng.range(0.0, 10.0);
        event += 1;
    }
    trace
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::unhex;

    // Core 5.4, Vol 6 Part C 4.2.1: ADV_NONCONN_IND on channel 38.
    const SPEC_PDU: &str = "42 09 a6 a5 a4 a3 a2 c1 01 02 03";

    #[test]
    fn crc_matches_core_spec_sample() {
        assert_eq!(crc24(&unhex(SPEC_PDU).unwrap(), ADV_CRC_INIT), [0xad, 0xb4, 0xeb]);
    }

    #[test]
    fn crc_matches_independent_implementations() {
        // rf24ble-rs test vector.
        assert_eq!(crc24(b"Hello World", ADV_CRC_INIT), [0xb6, 0x8c, 0xb0]);
        // Scapy test vector: empty ADV_IND with a zero address.
        assert_eq!(crc24(&[0x00, 0x06, 0, 0, 0, 0, 0, 0], ADV_CRC_INIT), [0x5a, 0x39, 0x60]);
    }

    #[test]
    fn whole_packet_matches_core_spec_sample() {
        let pkt = assemble(&unhex(SPEC_PDU).unwrap(), 38, ADV_ACCESS_ADDRESS, ADV_CRC_INIT);
        assert_eq!(pkt.preamble, 0xAA);
        assert_eq!(pkt.access_address, [0xd6, 0xbe, 0x89, 0x8e]);
        assert_eq!(pkt.whitened, unhex("94 cc e2 85 fd 7d 43 4e 1a a7 ac ef cf a5").unwrap());
        // First on-air bits as printed in the spec: 01010101 01101011 ...
        let bits = crate::units::bits_lsb_first(&pkt.bytes());
        let text: String = bits[..16].iter().map(|b| if *b { '1' } else { '0' }).collect();
        assert_eq!(text, "0101010101101011");
    }

    #[test]
    fn data_pdu_matches_core_spec_sample() {
        // Vol 6 Part C 4.3.1: LL data PDU, channel 16, CRCInit 0xC4C181.
        let pkt = assemble(&unhex("16 05 01 02 03 04 05").unwrap(), 16, 0xAA08_192B, 0xC4_C181);
        assert_eq!(pkt.crc, [0x45, 0xd0, 0xd2]);
        assert_eq!(pkt.whitened, unhex("c6 9b 52 31 db be 9d 4d f4 19").unwrap());
    }

    #[test]
    fn receive_undoes_assemble_and_spots_bit_errors() {
        let pdu = unhex(SPEC_PDU).unwrap();
        let pkt = assemble(&pdu, 38, ADV_ACCESS_ADDRESS, ADV_CRC_INIT);
        let rx = receive(&pkt.whitened, 38, ADV_CRC_INIT).unwrap();
        assert!(rx.crc_ok());
        assert_eq!(rx.pdu, pdu);
        for bit in 0..pkt.whitened.len() * 8 {
            let mut bad = pkt.whitened.clone();
            bad[bit / 8] ^= 1 << (bit % 8);
            assert!(!receive(&bad, 38, ADV_CRC_INIT).unwrap().crc_ok(), "bit {bit}");
        }
        // Listening on the wrong channel scrambles everything.
        assert!(!receive(&pkt.whitened, 37, ADV_CRC_INIT).unwrap().crc_ok());
    }

    #[test]
    fn adv_pdu_round_trip_matches_spec_fields() {
        let pdu = AdvPdu::from_bytes(&unhex(SPEC_PDU).unwrap()).unwrap();
        assert_eq!(pdu.pdu_type, PduType::AdvNonconnInd);
        assert!(pdu.tx_add);
        assert_eq!(pdu.adv_a, [0xc1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6]);
        assert_eq!(pdu.adv_data, [1, 2, 3]);
        assert_eq!(pdu.to_bytes(), unhex(SPEC_PDU).unwrap());
    }

    #[test]
    fn ad_structures_round_trip_and_report_errors() {
        let ad = vec![AdStructure::flags(0x06), AdStructure::complete_name("Lab")];
        let bytes = build_ad(&ad);
        assert_eq!(bytes, [2, 1, 6, 4, 9, b'L', b'a', b'b']);
        assert_eq!(parse_ad(&bytes), (ad, None));
        assert!(parse_ad(&[5, 9, b'x']).1.is_some());
    }

    #[test]
    fn channel_frequencies() {
        assert_eq!([channel_mhz(37), channel_mhz(0), channel_mhz(10), channel_mhz(38)], [2402, 2404, 2424, 2426]);
        assert_eq!([channel_mhz(11), channel_mhz(36), channel_mhz(39)], [2428, 2478, 2480]);
    }

    #[test]
    fn csa1_visits_every_channel_and_remaps_blocked_ones() {
        let mut all = Csa1::new(7, [true; 37]);
        let seen: std::collections::BTreeSet<u8> = (0..37).map(|_| all.next_hop().channel).collect();
        assert_eq!(seen.len(), 37);
        assert_eq!(Csa1::new(7, [true; 37]).next_hop().channel, 7);

        let mut used = [true; 37];
        (0..12).for_each(|c| used[c] = false);
        let mut some = Csa1::new(7, used);
        for _ in 0..100 {
            let hop = some.next_hop();
            assert!(hop.channel >= 12, "{hop:?}");
            assert_eq!(hop.remapped, hop.unmapped < 12);
        }
    }

    #[test]
    fn continuous_scanning_hears_the_first_event() {
        let p = AdvScanParams {
            adv_interval_ms: 100.0,
            packet_ms: 0.376,
            channel_gap_ms: 0.4,
            scan_interval_ms: 100.0,
            scan_window_ms: 100.0,
            scan_offset_ms: 0.0,
            duration_ms: 1000.0,
            seed: 1,
        };
        let trace = simulate_adv_scan(&p);
        assert!(trace.first_heard_ms.unwrap() < 5.0);
        // advDelay keeps event spacing between advInterval and advInterval + 10 ms.
        let starts: Vec<f64> = trace.packets.iter().filter(|x| x.channel == 37).map(|x| x.start_ms).collect();
        for w in starts.windows(2) {
            assert!((100.0..=110.0).contains(&(w[1] - w[0])));
        }
        // With the receiver off, nothing is heard.
        let deaf = simulate_adv_scan(&AdvScanParams { scan_window_ms: 0.0, ..p });
        assert!(deaf.first_heard_ms.is_none());
    }
}
