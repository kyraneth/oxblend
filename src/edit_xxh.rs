/*
xxHash algorithm by Yann Collet (BSD 2-Clause, notice retained below because the
constants and structure below implement the public XXH64 specification).

Implementation follows the public XXH64 algorithm specification. Tests cover
canonical python-xxhash vectors and split-at-every-position chunk independence.

Copyright (c) 2012-2021 Yann Collet
All rights reserved.

BSD 2-Clause License (https://www.opensource.org/licenses/bsd-license.php)

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

* Redistributions of source code must retain the above copyright notice, this
  list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright notice,
  this list of conditions and the following disclaimer in the documentation
  and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR
ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
(INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
*/

/// XXH64 with seed zero, streaming. Input bytes are consumed as the
/// specification's little-endian words regardless of host endianness; the
/// seek-frame field takes `digest() as u32` (least significant 32 bits) in the
/// container-defined byte order at its own site.
pub(crate) struct Xxh64 {
    v1: u64,
    v2: u64,
    v3: u64,
    v4: u64,
    total: u64,
    mem: [u8; 32],
    filled: usize,
}
impl Xxh64 {
    const PRIME1: u64 = 0x9E37_79B1_85EB_CA87;
    const PRIME2: u64 = 0xC2B2_AE3D_27D4_EB4F;
    const PRIME3: u64 = 0x1656_67B1_9E37_79F9;
    const PRIME4: u64 = 0x85EB_CA77_C2B2_AE63;
    const PRIME5: u64 = 0x27D4_EB2F_1656_67C5;

    /// One lane step of the block round.
    #[inline]
    fn lane(acc: u64, value: u64) -> u64 {
        acc.wrapping_add(value.wrapping_mul(Self::PRIME2))
            .rotate_left(31)
            .wrapping_mul(Self::PRIME1)
    }
    /// Merge one accumulated lane value into the running digest.
    #[inline]
    fn merge_round(acc: u64, value: u64) -> u64 {
        Self::lane(0, value) ^ acc
    }
    fn consume(v: [u64; 4], block: &[u8; 32]) -> [u64; 4] {
        [
            Self::lane(v[0], u64::from_le_bytes(block[0..8].try_into().unwrap())),
            Self::lane(v[1], u64::from_le_bytes(block[8..16].try_into().unwrap())),
            Self::lane(v[2], u64::from_le_bytes(block[16..24].try_into().unwrap())),
            Self::lane(v[3], u64::from_le_bytes(block[24..32].try_into().unwrap())),
        ]
    }

    pub fn new() -> Self {
        Self {
            v1: Self::PRIME1.wrapping_add(Self::PRIME2),
            v2: Self::PRIME2,
            v3: 0,
            v4: Self::PRIME1.wrapping_neg(),
            total: 0,
            mem: [0; 32],
            filled: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.filled + data.len() < 32 {
            self.mem[self.filled..self.filled + data.len()].copy_from_slice(data);
            self.filled += data.len();
            return;
        }
        if self.filled > 0 {
            let take = 32 - self.filled;
            self.mem[self.filled..].copy_from_slice(&data[..take]);
            let lanes = Self::consume([self.v1, self.v2, self.v3, self.v4], &self.mem);
            (self.v1, self.v2, self.v3, self.v4) = (lanes[0], lanes[1], lanes[2], lanes[3]);
            data = &data[take..];
            self.filled = 0;
        }
        let (blocks, rest) = data.as_chunks::<32>();
        for block in blocks {
            let lanes = Self::consume([self.v1, self.v2, self.v3, self.v4], block);
            (self.v1, self.v2, self.v3, self.v4) = (lanes[0], lanes[1], lanes[2], lanes[3]);
        }
        self.mem[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    pub fn digest(&self) -> u64 {
        let mut acc = if self.total >= 32 {
            let mut acc = self
                .v1
                .rotate_left(1)
                .wrapping_add(self.v2.rotate_left(7))
                .wrapping_add(self.v3.rotate_left(12))
                .wrapping_add(self.v4.rotate_left(18));
            for value in [self.v1, self.v2, self.v3, self.v4] {
                acc = Self::merge_round(acc, value)
                    .wrapping_mul(Self::PRIME1)
                    .wrapping_add(Self::PRIME4);
            }
            acc
        } else {
            Self::PRIME5
        };
        acc = acc.wrapping_add(self.total);

        let tail = &self.mem[..self.filled];
        let (words, tail) = tail.as_chunks::<8>();
        for word in words {
            acc = Self::merge_round(acc, u64::from_le_bytes(*word))
                .rotate_left(27)
                .wrapping_mul(Self::PRIME1)
                .wrapping_add(Self::PRIME4);
        }
        let (halfwords, tail) = tail.as_chunks::<4>();
        if let Some(halfword) = halfwords.first() {
            acc ^= (u32::from_le_bytes(*halfword) as u64).wrapping_mul(Self::PRIME1);
            acc = acc
                .rotate_left(23)
                .wrapping_mul(Self::PRIME2)
                .wrapping_add(Self::PRIME3);
        }
        for byte in tail {
            acc ^= (*byte as u64).wrapping_mul(Self::PRIME5);
            acc = acc.rotate_left(11).wrapping_mul(Self::PRIME1);
        }

        acc ^= acc >> 33;
        acc = acc.wrapping_mul(Self::PRIME2);
        acc ^= acc >> 29;
        acc = acc.wrapping_mul(Self::PRIME3);
        acc ^= acc >> 32;
        acc
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn digest_parts(parts: &[&[u8]]) -> u64 {
        let mut h = Xxh64::new();
        for part in parts {
            h.update(part);
        }
        h.digest()
    }

    #[test]
    fn known_vectors() {
        for (data, expected) in [
            (b"".as_slice(), 0xef46db3751d8e999),
            (b"a", 0xd24ec4f1a98c6e5b),
            (b"abc", 0x44bc2cf5ad770999),
        ] {
            let mut h = Xxh64::new();
            h.update(data);
            assert_eq!(h.digest(), expected, "data={data:?}");
        }
    }

    /// The seek-frame field carries the least significant 32 bits (xxhash.md).
    #[test]
    fn low32_field_value() {
        let mut h = Xxh64::new();
        h.update(b"");
        assert_eq!(h.digest() as u32, 0x51d8e999);
    }

    #[test]
    fn independent_canonical_vectors() {
        let arith_1k: Vec<u8> = (0..1024u64)
            .map(|i| (((i.wrapping_mul(31).wrapping_add((i / 11) * 7)) ^ (i >> 2)) & 0xff) as u8)
            .collect();
        let mut h = Xxh64::new();
        h.update(&arith_1k);
        assert_eq!(h.digest(), 0xeba2_a253_584c_e1f6);

        let mod_251 = (0..8193usize).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let mut h = Xxh64::new();
        h.update(&mod_251);
        assert_eq!(h.digest(), 0x755e_4bef_d10c_ccf4);

        let mut state = 0x1234_5678_9abc_def0u64;
        let mut lcg_1m = Vec::with_capacity(1 << 20);
        for _ in 0..(1 << 20) {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            lcg_1m.push((state >> 33) as u8);
        }
        let mut h = Xxh64::new();
        h.update(&lcg_1m);
        assert_eq!(h.digest(), 0x72ad_229f_910e_517d);
    }

    #[test]
    fn chunk_independence() {
        let data = (0..8193).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let mut whole = Xxh64::new();
        whole.update(&data);
        for size in [1, 3, 7, 31, 32, 33, 64, 128, 8192] {
            let mut h = Xxh64::new();
            for chunk in data.chunks(size) {
                h.update(chunk);
            }
            assert_eq!(h.digest(), whole.digest(), "chunk size {size}");
        }
    }

    #[test]
    fn split_at_every_position() {
        let data: [u8; 64] = [
            0xaa, 0x40, 0x43, 0xbb, 0x81, 0x52, 0x2a, 0x09, 0xa4, 0xef, 0x26, 0x11, 0xb3, 0x16,
            0x4d, 0xfc, 0x5f, 0x40, 0xc3, 0x52, 0xb4, 0x40, 0xb2, 0xbb, 0x15, 0xac, 0x22, 0x68,
            0xf7, 0xfb, 0xa9, 0x52, 0x49, 0xc5, 0xfd, 0x72, 0xcd, 0x01, 0xa4, 0xb7, 0x4d, 0x3a,
            0x3e, 0xdb, 0x35, 0x6d, 0x14, 0x5c, 0x46, 0xc7, 0x58, 0x97, 0x3d, 0xa5, 0x03, 0x15,
            0xfc, 0x43, 0x91, 0x06, 0x39, 0xe8, 0x59, 0xd4,
        ];
        let whole = digest_parts(&[&data]);
        assert_eq!(whole, 0xdad6_b230_52e7_c188);
        for split in 0..=data.len() {
            assert_eq!(
                digest_parts(&[&data[..split], &data[split..]]),
                whole,
                "split at {split}"
            );
        }
        // The same bytes as three updates at every pair of positions.
        for first in 0..=data.len() {
            for second in first..=data.len() {
                assert_eq!(
                    digest_parts(&[&data[..first], &data[first..second], &data[second..]]),
                    whole,
                    "three-way split at {first}/{second}"
                );
            }
        }
    }

    #[test]
    fn boundary_tails_match_canonical_vectors() {
        let data: [u8; 33] = [
            0x9f, 0x89, 0xf7, 0xd3, 0xa0, 0xb0, 0x21, 0xf3, 0x4f, 0xb6, 0xce, 0xd1, 0x46, 0x1e,
            0x27, 0xe0, 0xcd, 0x77, 0xcc, 0xa5, 0x5e, 0x18, 0x47, 0x4b, 0x33, 0x94, 0x10, 0xbd,
            0x22, 0x59, 0xec, 0x67, 0xbb,
        ];
        assert_eq!(digest_parts(&[&data]), 0xb788_9c00_f5c0_07a4);
        for len in 0..=data.len() {
            let mut one_shot = Xxh64::new();
            one_shot.update(&data[..len]);
            let mut halves = Xxh64::new();
            halves.update(&data[..len / 2]);
            halves.update(&data[len / 2..len]);
            let mut bytewise = Xxh64::new();
            for byte in &data[..len] {
                bytewise.update(std::slice::from_ref(byte));
            }
            assert_eq!(one_shot.digest(), halves.digest(), "len {len}");
            assert_eq!(one_shot.digest(), bytewise.digest(), "len {len}");
        }
    }
}
