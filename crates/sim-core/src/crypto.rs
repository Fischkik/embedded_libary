//! The three cryptographic building blocks Bluetooth Mesh is made of:
//! AES-128 (FIPS 197), AES-CMAC (RFC 4493) and AES-CCM (RFC 3610).
//!
//! Written out in plain Rust so the demos can show every intermediate value.
//! This is teaching code: it is checked against the published test vectors, but it
//! makes no attempt to resist timing attacks. Do not copy it into a product.

use std::sync::OnceLock;

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1B } else { 0 }
}

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    p
}

/// The AES substitution box, computed from its definition (inverse in GF(2^8) followed
/// by an affine transform) rather than typed in as a table.
fn sbox() -> &'static [u8; 256] {
    static SBOX: OnceLock<[u8; 256]> = OnceLock::new();
    SBOX.get_or_init(|| {
        let mut s = [0u8; 256];
        for x in 0..=255u8 {
            let inv = if x == 0 { 0 } else { (1..=255u8).find(|&y| gmul(x, y) == 1).unwrap_or(0) };
            let mut r = inv;
            let mut b = inv;
            for _ in 0..4 {
                b = b.rotate_left(1);
                r ^= b;
            }
            s[x as usize] = r ^ 0x63;
        }
        s
    })
}

/// AES with a 128-bit key, encryption direction only (that is all CMAC and CCM need).
#[derive(Clone)]
pub struct Aes128 {
    round_keys: [[u8; 16]; 11],
}

impl Aes128 {
    pub fn new(key: &[u8; 16]) -> Self {
        let s = sbox();
        let mut w = [[0u8; 4]; 44];
        for i in 0..4 {
            w[i].copy_from_slice(&key[4 * i..4 * i + 4]);
        }
        let mut rcon = 1u8;
        for i in 4..44 {
            let mut t = w[i - 1];
            if i % 4 == 0 {
                t = [s[t[1] as usize] ^ rcon, s[t[2] as usize], s[t[3] as usize], s[t[0] as usize]];
                rcon = xtime(rcon);
            }
            for j in 0..4 {
                w[i][j] = w[i - 4][j] ^ t[j];
            }
        }
        let mut round_keys = [[0u8; 16]; 11];
        for r in 0..11 {
            for c in 0..4 {
                round_keys[r][4 * c..4 * c + 4].copy_from_slice(&w[4 * r + c]);
            }
        }
        Self { round_keys }
    }

    pub fn encrypt(&self, block: &[u8; 16]) -> [u8; 16] {
        let s = sbox();
        let mut st = *block;
        let add = |st: &mut [u8; 16], rk: &[u8; 16]| st.iter_mut().zip(rk).for_each(|(a, b)| *a ^= b);
        add(&mut st, &self.round_keys[0]);
        for round in 1..=10 {
            // SubBytes + ShiftRows (the state is stored column by column).
            let mut t = [0u8; 16];
            for c in 0..4 {
                for r in 0..4 {
                    t[4 * c + r] = s[st[4 * ((c + r) % 4) + r] as usize];
                }
            }
            if round < 10 {
                // MixColumns
                for c in 0..4 {
                    let a = [t[4 * c], t[4 * c + 1], t[4 * c + 2], t[4 * c + 3]];
                    t[4 * c] = xtime(a[0]) ^ xtime(a[1]) ^ a[1] ^ a[2] ^ a[3];
                    t[4 * c + 1] = a[0] ^ xtime(a[1]) ^ xtime(a[2]) ^ a[2] ^ a[3];
                    t[4 * c + 2] = a[0] ^ a[1] ^ xtime(a[2]) ^ xtime(a[3]) ^ a[3];
                    t[4 * c + 3] = xtime(a[0]) ^ a[0] ^ a[1] ^ a[2] ^ xtime(a[3]);
                }
            }
            st = t;
            add(&mut st, &self.round_keys[round]);
        }
        st
    }
}

/// Encrypt one 16-byte block. Bluetooth calls this function `e`.
pub fn aes_e(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    Aes128::new(key).encrypt(block)
}

fn dbl(b: [u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = (b[i] << 1) | if i < 15 { b[i + 1] >> 7 } else { 0 };
    }
    if b[0] & 0x80 != 0 {
        out[15] ^= 0x87;
    }
    out
}

/// AES-CMAC: a 16-byte fingerprint of `msg` that only someone with `key` can compute.
pub fn cmac(key: &[u8; 16], msg: &[u8]) -> [u8; 16] {
    let aes = Aes128::new(key);
    let k1 = dbl(aes.encrypt(&[0; 16]));
    let k2 = dbl(k1);
    let blocks = msg.len().div_ceil(16).max(1);
    let complete = !msg.is_empty() && msg.len() % 16 == 0;
    let mut x = [0u8; 16];
    for i in 0..blocks - 1 {
        for j in 0..16 {
            x[j] ^= msg[16 * i + j];
        }
        x = aes.encrypt(&x);
    }
    let tail = &msg[16 * (blocks - 1)..];
    let mut last = [0u8; 16];
    last[..tail.len()].copy_from_slice(tail);
    let pad_key = if complete {
        k1
    } else {
        last[tail.len()] = 0x80;
        k2
    };
    for j in 0..16 {
        x[j] ^= last[j] ^ pad_key[j];
    }
    aes.encrypt(&x)
}

fn ccm_counter_block(nonce: &[u8; 13], counter: u16) -> [u8; 16] {
    let mut a = [0u8; 16];
    a[0] = 0x01; // L - 1, with a 2-byte length field
    a[1..14].copy_from_slice(nonce);
    a[14..].copy_from_slice(&counter.to_be_bytes());
    a
}

fn ccm_keystream_xor(aes: &Aes128, nonce: &[u8; 13], data: &mut [u8]) {
    for (i, chunk) in data.chunks_mut(16).enumerate() {
        let s = aes.encrypt(&ccm_counter_block(nonce, i as u16 + 1));
        chunk.iter_mut().zip(s).for_each(|(d, k)| *d ^= k);
    }
}

fn ccm_tag(aes: &Aes128, nonce: &[u8; 13], aad: &[u8], plaintext: &[u8], mic_len: usize) -> [u8; 16] {
    let mut b0 = [0u8; 16];
    b0[0] = (if aad.is_empty() { 0 } else { 0x40 }) | (((mic_len as u8 - 2) / 2) << 3) | 0x01;
    b0[1..14].copy_from_slice(nonce);
    b0[14..].copy_from_slice(&(plaintext.len() as u16).to_be_bytes());
    let mut x = aes.encrypt(&b0);
    let mut absorb = |data: &[u8]| {
        for chunk in data.chunks(16) {
            chunk.iter().enumerate().for_each(|(j, b)| x[j] ^= b);
            x = aes.encrypt(&x);
        }
    };
    if !aad.is_empty() {
        let mut framed = (aad.len() as u16).to_be_bytes().to_vec();
        framed.extend_from_slice(aad);
        absorb(&framed);
    }
    absorb(plaintext);
    let s0 = aes.encrypt(&ccm_counter_block(nonce, 0));
    x.iter_mut().zip(s0).for_each(|(a, b)| *a ^= b);
    x
}

/// AES-CCM: encrypt `plaintext` and append a message integrity check (MIC) of
/// `mic_len` bytes (4 or 8 in Bluetooth Mesh). Returns ciphertext followed by MIC.
pub fn ccm_encrypt(key: &[u8; 16], nonce: &[u8; 13], aad: &[u8], plaintext: &[u8], mic_len: usize) -> Vec<u8> {
    let aes = Aes128::new(key);
    let tag = ccm_tag(&aes, nonce, aad, plaintext, mic_len);
    let mut out = plaintext.to_vec();
    ccm_keystream_xor(&aes, nonce, &mut out);
    out.extend_from_slice(&tag[..mic_len]);
    out
}

/// Decrypt and check the MIC. Returns `None` if the MIC does not match, which means
/// the key is wrong or the message was changed.
pub fn ccm_decrypt(key: &[u8; 16], nonce: &[u8; 13], aad: &[u8], data: &[u8], mic_len: usize) -> Option<Vec<u8>> {
    if data.len() < mic_len {
        return None;
    }
    let aes = Aes128::new(key);
    let (ciphertext, mic) = data.split_at(data.len() - mic_len);
    let mut plain = ciphertext.to_vec();
    ccm_keystream_xor(&aes, nonce, &mut plain);
    let tag = ccm_tag(&aes, nonce, aad, &plain, mic_len);
    (tag[..mic_len] == *mic).then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::unhex;

    fn arr<const N: usize>(h: &str) -> [u8; N] {
        unhex(h).unwrap().try_into().unwrap()
    }

    #[test]
    fn aes_fips197_example() {
        let out = aes_e(&arr("000102030405060708090a0b0c0d0e0f"), &arr("00112233445566778899aabbccddeeff"));
        assert_eq!(out, arr::<16>("69c4e0d86a7b0430d8cdb78070b4c55a"));
    }

    #[test]
    fn cmac_rfc4493_examples() {
        let key = arr("2b7e151628aed2a6abf7158809cf4f3c");
        assert_eq!(cmac(&key, &[]), arr::<16>("bb1d6929e95937287fa37d129b756746"));
        let m16 = unhex("6bc1bee22e409f96e93d7e117393172a").unwrap();
        assert_eq!(cmac(&key, &m16), arr::<16>("070a16b46b4d4144f79bdd9dd04a287c"));
    }

    #[test]
    fn ccm_matches_mesh_spec_message_1() {
        // Mesh Protocol 1.1, 8.3.1: network encryption of a Friend Request.
        let key = arr("0953fa93e7caac9638f58820220a398e");
        let nonce = arr("00800000011201000012345678");
        let plain = unhex("fffd034b50057e400000010000").unwrap();
        let out = ccm_encrypt(&key, &nonce, &[], &plain, 8);
        assert_eq!(out, unhex("b5e5bfdacbaf6cb7fb6bff871f035444ce83a670df").unwrap());
        assert_eq!(ccm_decrypt(&key, &nonce, &[], &out, 8).unwrap(), plain);
        let mut tampered = out.clone();
        tampered[0] ^= 1;
        assert!(ccm_decrypt(&key, &nonce, &[], &tampered, 8).is_none());
    }

    #[test]
    fn ccm_with_additional_data_round_trips() {
        let key = [7u8; 16];
        let nonce = [9u8; 13];
        let out = ccm_encrypt(&key, &nonce, b"label uuid 16 by", b"hello mesh, longer than a block!", 4);
        assert_eq!(ccm_decrypt(&key, &nonce, b"label uuid 16 by", &out, 4).unwrap(), b"hello mesh, longer than a block!");
        assert!(ccm_decrypt(&key, &nonce, b"other", &out, 4).is_none());
    }
}
