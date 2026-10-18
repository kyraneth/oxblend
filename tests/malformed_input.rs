//! Committed malformed-input mutation and structure-aware fuzz target.
//!
//! The probe and seeded mutation sweep run inside qualification rather than
//! remaining a one-off measurement.
//! This target is deterministic and stable-toolchain only (no cargo-fuzz, no
//! nightly, no network): the corpus is derived from the on-disk BHead grammar,
//! so any host reproduces the same cases byte for byte.
//!
//! Contract asserted for every case, in process for the reader and through the
//! real CLI for the commands:
//!
//! 1. it terminates;
//! 2. it does not panic (`catch_unwind` for the reader, process exit code and
//!    structured JSON stderr for the CLI);
//! 3. a case that violates the grammar is not accepted; and
//! 4. an accepted case is structurally self-consistent: an independent layout
//!    walk (below) agrees with the parsed block chain.
//!
//! The corpus size is printed so qualification can see that the sweep ran. One
//! input can also be driven by an external stable-toolchain fuzzer:
//!
//! ```sh
//! OXBLEND_FUZZ_INPUT=path/to/input \
//!   cargo test --offline --locked --test malformed_input structure_aware_mutants -- --nocapture
//! ```

use oxblend::BlendFile;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Random cases per seed. This is the corpus definition below (the asserted
/// corpus size is derived from it), not a work budget.
const RANDOM_CASES_PER_SEED: usize = 400;

// ---------------------------------------------------------------- generator

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn below(state: &mut u64, n: usize) -> usize {
    (splitmix64(state) % n as u64) as usize
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Expect {
    /// Grammar-conforming: the reader must accept it.
    Accept,
    /// Grammar-violating: the reader must refuse it.
    Reject,
    /// May be accepted: random damage can land inside payload bytes.
    Either,
}

struct Case {
    name: String,
    bytes: Vec<u8>,
    expect: Expect,
}

// ------------------------------------------------- independent layout walk

/// A BHead as the wire format declares it, with the byte offsets of the fields
/// a structure-aware mutator needs to reach.
#[derive(Clone, Copy)]
struct Head {
    offset: usize,
    width: usize,
    code: [u8; 4],
    size_off: usize,
    size_len: usize,
    sdna_off: usize,
    count_off: usize,
    count_len: usize,
    payload_off: usize,
    size: usize,
}

struct Layout {
    little: bool,
    heads: Vec<Head>,
    endb: Head,
}

/// Independent re-implementation of the block-chain grammar, used both to
/// generate mutations and to cross-check anything the reader accepts.
fn parse_layout(bytes: &[u8]) -> Option<Layout> {
    let header = parse_header(bytes)?;
    let width = if header.format == 1 {
        32
    } else {
        16 + header.pointer
    };
    let mut heads: Vec<Head> = Vec::new();
    let mut position = header.len;
    loop {
        let raw = bytes.get(position..position.checked_add(width)?)?;
        let code: [u8; 4] = raw.get(..4)?.try_into().ok()?;
        let (size, count, size_off, size_len, sdna_off, count_off, count_len) =
            if header.format == 1 {
                (
                    read_uint(raw, 16, 8, header.little)?,
                    read_uint(raw, 24, 8, header.little)?,
                    position + 16,
                    8usize,
                    position + 4,
                    position + 24,
                    8usize,
                )
            } else {
                (
                    read_uint(raw, 4, 4, header.little)?,
                    read_uint(raw, 12 + header.pointer, 4, header.little)?,
                    position + 4,
                    4usize,
                    position + 8 + header.pointer,
                    position + 12 + header.pointer,
                    4usize,
                )
            };
        let signed_max = if size_len == 4 {
            i32::MAX as u64
        } else {
            i64::MAX as u64
        };
        if size > signed_max || count > signed_max {
            return None;
        }
        let payload_off = position + width;
        let remaining = bytes.len().checked_sub(payload_off)?;
        if size > remaining as u64 {
            return None;
        }
        let head = Head {
            offset: position,
            width,
            code,
            size_off,
            size_len,
            sdna_off,
            count_off,
            count_len,
            payload_off,
            size: size as usize,
        };
        if &code == b"ENDB" {
            if size != 0 || count != 0 || payload_off != bytes.len() {
                return None;
            }
            return Some(Layout {
                little: header.little,
                heads,
                endb: head,
            });
        }
        if &code == b"DNA1" && heads.iter().any(|h| &h.code == b"DNA1") {
            return None;
        }
        position = payload_off.checked_add(size as usize)?;
        heads.push(head);
    }
}

struct Header {
    len: usize,
    little: bool,
    pointer: usize,
    format: u32,
}

fn parse_header(data: &[u8]) -> Option<Header> {
    if data.len() < 12 || !data.starts_with(b"BLENDER") {
        return None;
    }
    let indicator = data[7];
    if indicator == b'_' || indicator == b'-' {
        let endian = *data.get(8)?;
        if endian != b'v' && endian != b'V' {
            return None;
        }
        digits(data.get(9..12)?)?;
        Some(Header {
            len: 12,
            little: endian == b'v',
            pointer: if indicator == b'_' { 4 } else { 8 },
            format: 0,
        })
    } else {
        if data.len() < 17 || data.get(7..13)? != b"17-01v".as_slice() {
            return None;
        }
        digits(data.get(13..17)?)?;
        Some(Header {
            len: 17,
            little: true,
            pointer: 8,
            format: 1,
        })
    }
}

fn digits(data: &[u8]) -> Option<()> {
    if data.is_empty() || !data.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(())
}

fn read_uint(data: &[u8], offset: usize, len: usize, little: bool) -> Option<u64> {
    let part = data.get(offset..offset.checked_add(len)?)?;
    let mut value = 0u64;
    if little {
        for (i, byte) in part.iter().enumerate() {
            value |= u64::from(*byte) << (i * 8);
        }
    } else {
        for byte in part {
            value = (value << 8) | u64::from(*byte);
        }
    }
    Some(value)
}

fn write_uint(data: &mut [u8], offset: usize, len: usize, little: bool, value: u64) {
    for i in 0..len {
        let byte = ((value >> (8 * i)) & 0xff) as u8;
        let at = if little {
            offset + i
        } else {
            offset + len - 1 - i
        };
        data[at] = byte;
    }
}

// ------------------------------------------------------------ seed material

fn fixture(relative: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn dna_payload() -> Vec<u8> {
    fn align(dna: &mut Vec<u8>) {
        while !dna.len().is_multiple_of(4) {
            dna.push(0);
        }
    }
    let mut dna = b"SDNANAME".to_vec();
    dna.extend(3u32.to_le_bytes());
    dna.extend(b"value\0pad\0*next\0");
    align(&mut dna);
    dna.extend(b"TYPE");
    dna.extend(2u32.to_le_bytes());
    dna.extend(b"int\0Node\0");
    align(&mut dna);
    dna.extend(b"TLEN");
    dna.extend(4u16.to_le_bytes());
    dna.extend(16u16.to_le_bytes());
    dna.extend(b"STRC");
    dna.extend(1u32.to_le_bytes());
    for n in [1u16, 3, 0, 0, 0, 1, 1, 2] {
        dna.extend(n.to_le_bytes());
    }
    dna
}

fn legacy_head(out: &mut Vec<u8>, code: &[u8; 4], address: u64, data: &[u8], count: u32) {
    out.extend(code);
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(address.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(data);
}

fn format1_head(out: &mut Vec<u8>, code: &[u8; 4], address: u64, data: &[u8], count: u64) {
    out.extend(code);
    out.extend(0u32.to_le_bytes());
    out.extend(address.to_le_bytes());
    out.extend((data.len() as u64).to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(data);
}

/// Minimal 4.x legacy file: 24-byte BHeads, one SDNA struct.
fn synthetic_legacy() -> Vec<u8> {
    let record = |value: u32, next: u64| {
        let mut r = value.to_le_bytes().to_vec();
        r.extend(0u32.to_le_bytes());
        r.extend(next.to_le_bytes());
        r
    };
    let mut out = b"BLENDER-v405".to_vec();
    legacy_head(&mut out, b"AA\0\0", 0x1000, &record(1, 0x2000), 1);
    let mut two = record(2, 0);
    two.extend(record(3, 0x1000));
    legacy_head(&mut out, b"BB\0\0", 0x2000, &two, 2);
    legacy_head(&mut out, b"AA\0\0", 0x3000, &record(4, 0), 1);
    legacy_head(&mut out, b"DNA1", 0, &dna_payload(), 1);
    legacy_head(&mut out, b"ENDB", 0, &[], 0);
    out
}

/// Minimal `BLENDER17-01v` file: 32-byte BHeads, the other declared width.
fn synthetic_format1() -> Vec<u8> {
    let mut out = b"BLENDER17-01v0451".to_vec();
    format1_head(&mut out, b"AA\0\0", 0x1000, &[0u8; 16], 1);
    format1_head(&mut out, b"DNA1", 0, &dna_payload(), 1);
    format1_head(&mut out, b"ENDB", 0, &[], 0);
    out
}

// -------------------------------------------------------- structural cases

fn structural_cases(seed: &str, base: &[u8], layout: &Layout, structs: usize) -> Vec<Case> {
    let mut cases = Vec::new();
    let reject = |cases: &mut Vec<Case>, name: String, bytes: Vec<u8>| {
        cases.push(Case {
            name,
            bytes,
            expect: Expect::Reject,
        });
    };

    // Truncation at every structural boundary, plus the last byte of the file.
    let mut cuts = vec![layout.heads.first().map_or(0, |h| h.offset)];
    for head in &layout.heads {
        cuts.push(head.offset);
        cuts.push(head.offset + head.width - 1);
        cuts.push(head.payload_off);
        if head.size > 0 {
            cuts.push(head.payload_off + head.size - 1);
        }
    }
    cuts.push(layout.endb.offset);
    cuts.sort_unstable();
    cuts.dedup();
    for cut in cuts {
        if cut < base.len() {
            reject(
                &mut cases,
                format!("{seed}/truncate@{cut}"),
                base[..cut].to_vec(),
            );
        }
    }

    // Declared extents: past end of file, or unrepresentable signed values.
    for (i, head) in layout.heads.iter().enumerate() {
        let remaining = base.len() - head.payload_off;
        let values = [
            remaining as u64 + 1,
            if head.size_len == 4 {
                i32::MAX as u64
            } else {
                i64::MAX as u64
            },
            if head.size_len == 4 {
                i32::MAX as u64 + 1
            } else {
                i64::MAX as u64 + 1
            },
        ];
        for value in values {
            let mut bytes = base.to_vec();
            write_uint(
                &mut bytes,
                head.size_off,
                head.size_len,
                layout.little,
                value,
            );
            reject(&mut cases, format!("{seed}/head{i}/size={value}"), bytes);
        }
        let mut bytes = base.to_vec();
        write_uint(
            &mut bytes,
            head.size_off,
            head.size_len,
            layout.little,
            head.size as u64 + 1,
        );
        cases.push(Case {
            name: format!("{seed}/head{i}/size-in-bounds+1"),
            bytes,
            expect: Expect::Either,
        });
    }

    // SDNA index out of range for a non-DNA1 block, and a negative encoding.
    for (i, head) in layout.heads.iter().enumerate() {
        if &head.code == b"DNA1" {
            continue;
        }
        for value in [structs as u64, i32::MAX as u64 + 1] {
            let mut bytes = base.to_vec();
            write_uint(&mut bytes, head.sdna_off, 4, layout.little, value);
            reject(&mut cases, format!("{seed}/head{i}/sdna={value}"), bytes);
        }
    }

    // Code-level structural violations.
    if let Some(dna) = layout.heads.iter().find(|h| &h.code == b"DNA1") {
        let mut bytes = base.to_vec();
        bytes[dna.offset..dna.offset + 4].copy_from_slice(b"DNX1");
        reject(&mut cases, format!("{seed}/dna1-renamed"), bytes);
        let mut bytes = base.to_vec();
        bytes[dna.offset..dna.offset + 4].copy_from_slice(b"ENDB");
        reject(&mut cases, format!("{seed}/dna1-renamed-to-endb"), bytes);
    }
    let mut bytes = base.to_vec();
    bytes[layout.endb.offset..layout.endb.offset + 4].copy_from_slice(b"ENDX");
    reject(&mut cases, format!("{seed}/endb-renamed"), bytes);
    for (i, head) in layout.heads.iter().enumerate() {
        if &head.code == b"DNA1" {
            continue;
        }
        let mut bytes = base.to_vec();
        bytes[head.offset..head.offset + 4].copy_from_slice(b"DNA1");
        reject(&mut cases, format!("{seed}/head{i}/second-dna1"), bytes);
        let mut zeroed = base.to_vec();
        zeroed[head.offset..head.offset + 4].copy_from_slice(b"\0\0\0\0");
        cases.push(Case {
            name: format!("{seed}/head{i}/zero-code"),
            bytes: zeroed,
            expect: Expect::Either,
        });
    }

    // ENDB invariants and trailing bytes.
    for (what, offset, value) in [
        ("size", layout.endb.size_off, 1u64),
        ("count", layout.endb.count_off, 1u64),
    ] {
        let mut bytes = base.to_vec();
        write_uint(
            &mut bytes,
            offset,
            if what == "size" {
                layout.endb.size_len
            } else {
                layout.endb.count_len
            },
            layout.little,
            value,
        );
        reject(&mut cases, format!("{seed}/endb-{what}"), bytes);
    }
    let mut bytes = base.to_vec();
    bytes.push(0);
    reject(&mut cases, format!("{seed}/bytes-after-endb"), bytes);

    // Header-level violations.
    for at in 0..7 {
        let mut bytes = base.to_vec();
        bytes[at] ^= 0x20;
        reject(&mut cases, format!("{seed}/magic-{at}"), bytes);
    }
    let mut bytes = base.to_vec();
    bytes[9] = b'x';
    reject(&mut cases, format!("{seed}/version-not-decimal"), bytes);
    if parse_header(base).is_some_and(|h| h.format == 0) {
        let mut bytes = base.to_vec();
        bytes[7] = if base[7] == b'-' { b'_' } else { b'-' };
        reject(&mut cases, format!("{seed}/pointer-indicator"), bytes);
        let mut bytes = base.to_vec();
        bytes[8] = if base[8] == b'v' { b'V' } else { b'v' };
        reject(&mut cases, format!("{seed}/endian-marker"), bytes);
    } else {
        let mut bytes = base.to_vec();
        bytes[7] = b'-';
        reject(&mut cases, format!("{seed}/format-indicator"), bytes);
    }

    cases
}

fn random_cases(seed: &str, base: &[u8], state: &mut u64) -> Vec<Case> {
    let mut cases = Vec::with_capacity(RANDOM_CASES_PER_SEED);
    for i in 0..RANDOM_CASES_PER_SEED {
        let mut bytes = base.to_vec();
        let expect = match below(state, 6) {
            0 => {
                let at = below(state, bytes.len());
                bytes[at] ^= 1 << below(state, 8);
                Expect::Either
            }
            1 => {
                let at = below(state, bytes.len());
                let len = 1 + below(state, 8);
                for k in 0..len {
                    if let Some(slot) = bytes.get_mut(at + k) {
                        *slot = splitmix64(state) as u8;
                    }
                }
                Expect::Either
            }
            2 => {
                let at = below(state, bytes.len());
                bytes.truncate(at);
                Expect::Reject
            }
            3 => {
                let at = below(state, bytes.len());
                let end = (at + 1 + below(state, 32)).min(bytes.len());
                bytes.drain(at..end);
                Expect::Either
            }
            4 => {
                let at = below(state, bytes.len());
                let len = (1 + below(state, 32)).min(bytes.len() - at);
                let span = bytes[at..at + len].to_vec();
                let to = below(state, bytes.len());
                bytes.splice(to..to, span);
                Expect::Either
            }
            _ => {
                let at = below(state, bytes.len().saturating_sub(8).max(1));
                let value = splitmix64(state);
                for k in 0..8 {
                    if let Some(slot) = bytes.get_mut(at + k) {
                        *slot = ((value >> (8 * k)) & 0xff) as u8;
                    }
                }
                Expect::Either
            }
        };
        cases.push(Case {
            name: format!("{seed}/random{i}"),
            bytes,
            expect,
        });
    }
    cases
}

fn corpus() -> Vec<Case> {
    let seeds = [
        ("legacy-synthetic", synthetic_legacy()),
        ("format1-synthetic", synthetic_format1()),
        ("native-cross-id", fixture("trace-owner/cross-id.blend")),
    ];
    let mut cases = Vec::new();
    let mut state = 0x5eed_1234_abcd_9876u64;
    for (name, bytes) in seeds {
        let layout = parse_layout(&bytes)
            .unwrap_or_else(|| panic!("{name}: seed is not a complete block chain"));
        let file = BlendFile::from_bytes(bytes.clone())
            .unwrap_or_else(|e| panic!("{name}: seed does not parse: {e}"));
        let structs = file.schema().structs.len();
        cases.push(Case {
            name: format!("{name}/valid"),
            bytes: bytes.clone(),
            expect: Expect::Accept,
        });
        cases.extend(structural_cases(name, &bytes, &layout, structs));
        cases.extend(random_cases(name, &bytes, &mut state));
    }
    assert!(
        cases.len() > RANDOM_CASES_PER_SEED * 3,
        "corpus collapsed to {} cases",
        cases.len()
    );
    cases
}

// --------------------------------------------------------------- reader run

fn parse_quiet(bytes: &[u8]) -> Result<BlendFile, String> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        BlendFile::from_bytes(bytes.to_vec())
    }));
    std::panic::set_hook(hook);
    match outcome {
        Ok(Ok(file)) => Ok(file),
        Ok(Err(error)) => Err(format!("{}: {}", error.kind, error.message)),
        Err(_) => Err("PANIC".to_string()),
    }
}

fn consistent(bytes: &[u8], file: &BlendFile) -> Result<(), String> {
    let layout = parse_layout(bytes)
        .ok_or_else(|| "accepted bytes have no complete BHead chain".to_string())?;
    if layout.endb.payload_off != bytes.len() {
        return Err("accepted bytes have no final ENDB".to_string());
    }
    if !layout.heads.iter().any(|h| &h.code == b"DNA1") {
        return Err("accepted bytes have no DNA1".to_string());
    }
    if layout.heads.len() != file.blocks().len() {
        return Err(format!(
            "layout has {} heads, reader has {} blocks",
            layout.heads.len(),
            file.blocks().len()
        ));
    }
    for (head, block) in layout.heads.iter().zip(file.blocks()) {
        if head.code != block.code
            || head.size != block.size
            || head.payload_off != block.data_offset
        {
            return Err(format!("head at {} disagrees with the reader", head.offset));
        }
    }
    if file.schema().structs.is_empty() {
        return Err("accepted bytes have an empty SDNA struct table".to_string());
    }
    Ok(())
}

#[test]
fn structure_aware_mutants_never_panic_and_never_accept_corrupt_input() {
    let cases = corpus();
    let mut accepted = 0usize;
    for case in &cases {
        match parse_quiet(&case.bytes) {
            Ok(file) => {
                assert_ne!(
                    case.expect,
                    Expect::Reject,
                    "{}: corrupt input accepted",
                    case.name
                );
                consistent(&case.bytes, &file)
                    .unwrap_or_else(|problem| panic!("{}: {}", case.name, problem));
                accepted += 1;
            }
            Err(reason) => {
                assert_ne!(reason, "PANIC", "{}: reader panicked", case.name);
                assert_ne!(
                    case.expect,
                    Expect::Accept,
                    "{}: valid seed rejected: {}",
                    case.name,
                    reason
                );
            }
        }
    }
    println!("MALFORMED_CASES={}", cases.len());
    println!("MALFORMED_ACCEPTED={accepted}");
    fuzz_target_single_input();
}

// ----------------------------------------------------------------- CLI run

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("oxblend-{name}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn cli_cases() -> Vec<Case> {
    let legacy = synthetic_legacy();
    let layout = parse_layout(&legacy).expect("synthetic seed layout");
    let file = BlendFile::from_bytes(legacy.clone()).expect("synthetic seed parses");
    let structs = file.schema().structs.len();
    let mut cases = Vec::new();
    let accept = |cases: &mut Vec<Case>, name: &str, bytes: Vec<u8>| {
        cases.push(Case {
            name: name.to_string(),
            bytes,
            expect: Expect::Accept,
        });
    };
    let reject = |cases: &mut Vec<Case>, name: &str, bytes: Vec<u8>| {
        cases.push(Case {
            name: name.to_string(),
            bytes,
            expect: Expect::Reject,
        });
    };

    accept(&mut cases, "legacy-valid", legacy.clone());
    accept(&mut cases, "format1-valid", synthetic_format1());
    accept(
        &mut cases,
        "native-valid",
        fixture("trace-owner/cross-id.blend"),
    );

    let head = layout.heads[0];
    reject(
        &mut cases,
        "truncated-mid-payload",
        legacy[..head.payload_off + 1].to_vec(),
    );
    let mut declared = legacy.clone();
    write_uint(
        &mut declared,
        head.size_off,
        head.size_len,
        layout.little,
        (legacy.len() - head.payload_off) as u64 + 1,
    );
    reject(&mut cases, "declared-size-past-eof", declared.clone());
    let mut sdna = legacy.clone();
    write_uint(&mut sdna, head.sdna_off, 4, layout.little, structs as u64);
    reject(&mut cases, "sdna-index-out-of-range", sdna);
    let dna = layout
        .heads
        .iter()
        .find(|h| &h.code == b"DNA1")
        .expect("DNA1");
    let mut renamed = legacy.clone();
    renamed[dna.offset..dna.offset + 4].copy_from_slice(b"DNX1");
    reject(&mut cases, "dna1-renamed", renamed);
    let mut no_end = legacy.clone();
    no_end[layout.endb.offset..layout.endb.offset + 4].copy_from_slice(b"ENDX");
    reject(&mut cases, "endb-renamed", no_end);
    let mut trailing = legacy.clone();
    trailing.push(0);
    reject(&mut cases, "bytes-after-endb", trailing);
    let mut endb_size = legacy.clone();
    write_uint(
        &mut endb_size,
        layout.endb.size_off,
        layout.endb.size_len,
        layout.little,
        1,
    );
    reject(&mut cases, "endb-nonzero-size", endb_size);
    let mut magic = legacy.clone();
    magic[2] ^= 0xff;
    reject(&mut cases, "bad-magic", magic);

    // Compressed containers: the same grammar behind gzip/zstd admission.
    accept(&mut cases, "gzip-valid", gzip(&legacy));
    let mut trailer = gzip(&legacy);
    let last = trailer.len() - 4;
    trailer[last] ^= 0xff;
    reject(&mut cases, "gzip-corrupt-trailer", trailer);
    reject(
        &mut cases,
        "gzip-not-a-blend",
        gzip(b"this is not a blend file"),
    );
    reject(&mut cases, "gzip-declared-size-past-eof", gzip(&declared));
    // Bounded by construction: the decoded header check fires before any tail
    // is materialised, so this asserts behaviour, not a memory budget.
    reject(
        &mut cases,
        "gzip-zero-bomb",
        gzip(&vec![0u8; 4 * 1024 * 1024]),
    );
    accept(
        &mut cases,
        "zstd-valid",
        zstd::stream::encode_all(legacy.as_slice(), 0).unwrap(),
    );
    reject(
        &mut cases,
        "zstd-not-a-blend",
        zstd::stream::encode_all(b"this is not a blend file".as_slice(), 0).unwrap(),
    );

    cases
}

#[test]
fn cli_refuses_corrupt_input_without_panicking() {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_oxblend"));
    let work = Scratch::new("malformed-cli");
    let cases = cli_cases();
    let mut runs = 0usize;
    for case in &cases {
        let path = work.0.join(case.name.replace('/', "_"));
        std::fs::write(&path, &case.bytes).unwrap();
        for command in ["summary", "list", "blocks", "trace"] {
            let output = Command::new(&binary)
                .arg(command)
                .arg(&path)
                .output()
                .unwrap();
            runs += 1;
            let code = output
                .status
                .code()
                .unwrap_or_else(|| panic!("{}/{command}: killed by a signal", case.name));
            assert_ne!(code, 101, "{}/{command}: panicked", case.name);
            if case.expect == Expect::Reject {
                assert_eq!(
                    code,
                    2,
                    "{}/{command}: corrupt input outcome (stdout {} bytes, stderr {})",
                    case.name,
                    output.stdout.len(),
                    String::from_utf8_lossy(&output.stderr)
                );
                let error: serde_json::Value = serde_json::from_slice(&output.stderr)
                    .unwrap_or_else(|e| {
                        panic!(
                            "{}/{command}: failure is not structured JSON: {e} ({})",
                            case.name,
                            String::from_utf8_lossy(&output.stderr)
                        )
                    });
                assert!(
                    error.get("error").is_some(),
                    "{}/{command}: {}",
                    case.name,
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                assert!(
                    matches!(code, 0 | 2),
                    "{}/{command}: unexpected exit {code}",
                    case.name
                );
            }
        }
    }
    println!("MALFORMED_CLI_CASES={}", cases.len());
    println!("MALFORMED_CLI_RUNS={runs}");
}

/// External stable-toolchain entry point, also exercised with a valid fixture
/// during the reader sweep when no external input is supplied:
///
/// ```sh
/// OXBLEND_FUZZ_INPUT=path/to/input \
///   cargo test --offline --locked --test malformed_input structure_aware_mutants -- --nocapture
/// ```
fn fuzz_target_single_input() {
    let path = std::env::var_os("OXBLEND_FUZZ_INPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/trace-owner/cross-id.blend")
        });
    let bytes = std::fs::read(&path).unwrap();
    match parse_quiet(&bytes) {
        Ok(file) => {
            consistent(&bytes, &file).expect("accepted input is not self-consistent");
            println!(
                "FUZZ_INPUT={} bytes={} accepted",
                path.display(),
                bytes.len()
            );
        }
        Err(reason) => {
            assert_ne!(reason, "PANIC", "{}: reader panicked", path.display());
            println!(
                "FUZZ_INPUT={} bytes={} refused ({reason})",
                path.display(),
                bytes.len()
            );
        }
    }
}
