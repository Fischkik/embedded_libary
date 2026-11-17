//! Bluetooth LE legacy pairing (Security Manager, Core Vol 3 Part H): the confirm value
//! function c1, the key function s1, and why a recorded pairing can be cracked.
//!
//! Teaching code, checked against the specification's sample data, not hardened.
//! 128-bit values are byte arrays with the most significant byte first, as the
//! specification writes them.

use crate::crypto::aes_e;

pub type Block = [u8; 16];

fn xor(a: &Block, b: &Block) -> Block {
    std::array::from_fn(|i| a[i] ^ b[i])
}

/// Everything about the two devices that goes into the confirm value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairingContext {
    /// The 7-byte Pairing Request and Pairing Response, as 56-bit numbers.
    pub preq: [u8; 7],
    pub pres: [u8; 7],
    /// Address types of the initiator and the responder (0 public, 1 random).
    pub iat: u8,
    pub rat: u8,
    /// The two 48-bit device addresses.
    pub ia: [u8; 6],
    pub ra: [u8; 6],
}

impl PairingContext {
    /// "p1 = pres || preq || rat' || iat'"
    pub fn p1(&self) -> Block {
        let mut p = [0u8; 16];
        p[..7].copy_from_slice(&self.pres);
        p[7..14].copy_from_slice(&self.preq);
        p[14] = self.rat & 1;
        p[15] = self.iat & 1;
        p
    }

    /// "p2 = padding || ia || ra", padding being 32 zero bits.
    pub fn p2(&self) -> Block {
        let mut p = [0u8; 16];
        p[4..10].copy_from_slice(&self.ia);
        p[10..].copy_from_slice(&self.ra);
        p
    }

    /// The values of the specification's own example (Core Vol 3 Part H 2.2.3).
    pub fn spec_example() -> Self {
        Self { preq: [0x07, 0x07, 0x10, 0x00, 0x00, 0x01, 0x01], pres: [0x05, 0x00, 0x08, 0x00, 0x00, 0x03, 0x02], iat: 1, rat: 0, ia: [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6], ra: [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6] }
    }
}

/// "c1(k, r, preq, pres, iat, rat, ia, ra) = e(k, e(k, r XOR p1) XOR p2)"
pub fn c1(k: &Block, r: &Block, ctx: &PairingContext) -> Block {
    aes_e(k, &xor(&aes_e(k, &xor(r, &ctx.p1())), &ctx.p2()))
}

/// "s1(k, r1, r2) = e(k, r')", r' being the low 64 bits of r1 followed by those of r2.
pub fn s1(k: &Block, r1: &Block, r2: &Block) -> Block {
    let mut r = [0u8; 16];
    r[..8].copy_from_slice(&r1[8..]);
    r[8..].copy_from_slice(&r2[8..]);
    aes_e(k, &r)
}

/// The temporary key: zero for Just Works, the 6-digit passkey in the low bits for
/// Passkey Entry ("passkey 019655 gives TK 0x...4CC7").
pub fn tk_from_passkey(passkey: u32) -> Block {
    let mut tk = [0u8; 16];
    tk[12..].copy_from_slice(&passkey.to_be_bytes());
    tk
}

/// One complete legacy pairing as it appears on the air, plus the key both sides end up with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pairing {
    pub ctx: PairingContext,
    pub mrand: Block,
    pub srand: Block,
    pub mconfirm: Block,
    pub sconfirm: Block,
    /// Not sent: computed by both sides. "STK = s1(TK, LP_RAND_R, LP_RAND_I)".
    pub stk: Block,
}

pub fn pair(ctx: PairingContext, passkey: u32, mrand: Block, srand: Block) -> Pairing {
    let tk = tk_from_passkey(passkey);
    Pairing { ctx, mrand, srand, mconfirm: c1(&tk, &mrand, &ctx), sconfirm: c1(&tk, &srand, &ctx), stk: s1(&tk, &srand, &mrand) }
}

/// An eavesdropper's search: try passkeys `from..to` against what was recorded. Returns
/// the passkey if one of them reproduces the initiator's confirm value.
pub fn crack(recorded: &Pairing, from: u32, to: u32) -> Option<u32> {
    (from..to.min(1_000_000)).find(|&p| c1(&tk_from_passkey(p), &recorded.mrand, &recorded.ctx) == recorded.mconfirm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::unhex;

    fn block(hex: &str) -> Block {
        unhex(hex).unwrap().try_into().unwrap()
    }

    #[test]
    fn c1_matches_the_specification_example() {
        let ctx = PairingContext::spec_example();
        assert_eq!(ctx.p1(), block("05000800000302070710000001010001"));
        assert_eq!(ctx.p2(), block("00000000A1A2A3A4A5A6B1B2B3B4B5B6"));
        let r = block("5783D52156AD6F0E6388274EC6702EE0");
        assert_eq!(c1(&[0; 16], &r, &ctx), block("1E1E3FEF878988EAD2A74DC5BEF13B86"));
    }

    #[test]
    fn s1_matches_the_specification_example() {
        let r1 = block("000F0E0D0C0B0A091122334455667788");
        let r2 = block("010203040506070899AABBCCDDEEFF00");
        assert_eq!(s1(&[0; 16], &r1, &r2), block("9a1fe1f0e8b0f49b5b4216ae796da062"));
    }

    #[test]
    fn passkey_encoding_and_cracking() {
        assert_eq!(tk_from_passkey(19_655), block("00000000000000000000000000004CC7"));
        let ctx = PairingContext::spec_example();
        let p = pair(ctx, 4_321, block("5783D52156AD6F0E6388274EC6702EE0"), block("000F0E0D0C0B0A091122334455667788"));
        // Both sides check each other's confirm with the same TK: it fits.
        assert_eq!(c1(&tk_from_passkey(4_321), &p.srand, &ctx), p.sconfirm);
        // The eavesdropper tries passkeys in order and finds it.
        assert_eq!(crack(&p, 0, 4_000), None);
        assert_eq!(crack(&p, 4_000, 5_000), Some(4_321));
        // ... and then computes the same short-term key.
        assert_eq!(s1(&tk_from_passkey(4_321), &p.srand, &p.mrand), p.stk);
        // Just Works: the first guess (TK = 0) is already right.
        let jw = pair(ctx, 0, p.mrand, p.srand);
        assert_eq!(crack(&jw, 0, 1), Some(0));
    }
}
