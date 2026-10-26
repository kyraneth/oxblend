//! Wire fixtures written from the OpenEXR file-layout specification, not our codec.
use serde_json::Value;
use std::{path::Path, process::Command};
mod support {
    pub mod scratch;
}
fn attr(b: &mut Vec<u8>, name: &str, kind: &str, v: &[u8]) {
    b.extend(name.bytes());
    b.push(0);
    b.extend(kind.bytes());
    b.push(0);
    b.extend((v.len() as i32).to_le_bytes());
    b.extend(v);
}
fn fixture() -> Vec<u8> {
    let mut b = 20000630u32.to_le_bytes().to_vec();
    b.extend(2u32.to_le_bytes());
    let mut channels = Vec::new();
    for name in ["Beauty.B", "Beauty.G", "Beauty.R", "Mist.Z"] {
        channels.extend(name.bytes());
        channels.push(0);
        channels.extend(2i32.to_le_bytes());
        channels.extend([0; 4]);
        channels.extend(1i32.to_le_bytes());
        channels.extend(1i32.to_le_bytes());
    }
    channels.push(0);
    attr(&mut b, "channels", "chlist", &channels);
    attr(&mut b, "compression", "compression", &[0]);
    for name in ["dataWindow", "displayWindow"] {
        attr(
            &mut b,
            name,
            "box2i",
            &[-2i32, 3, -1, 4]
                .into_iter()
                .flat_map(i32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    attr(&mut b, "lineOrder", "lineOrder", &[0]);
    attr(&mut b, "pixelAspectRatio", "float", &1f32.to_le_bytes());
    attr(&mut b, "screenWindowCenter", "v2f", &[0; 8]);
    attr(&mut b, "screenWindowWidth", "float", &1f32.to_le_bytes());
    b.push(0);
    let first = b.len() as u64 + 16;
    b.extend(first.to_le_bytes());
    b.extend((first + 40).to_le_bytes());
    for y in 0..2 {
        b.extend((3i32 + y).to_le_bytes());
        b.extend(32i32.to_le_bytes());
        for c in 0..4 {
            for x in 0..2 {
                b.extend((100. * c as f32 + 10. * y as f32 + x as f32).to_le_bytes());
            }
        }
    }
    b
}
fn run(input: &Path, output: &Path, pass: &str, part: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-pixels")
        .arg(input)
        .args(["--part", part, "--pass", pass, "--output"])
        .arg(output)
        .output()
        .unwrap()
}
#[test]
fn named_pass_is_explicit_ordered_and_never_first_pass_fallback() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    let p = d.join("wire.exr");
    std::fs::write(&p, fixture()).unwrap();
    let output = d.join("beauty.f32");
    let out = run(&p, &output, "Beauty", "0");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["channels"],
        serde_json::json!(["Beauty.B", "Beauty.G", "Beauty.R"])
    );
    assert_eq!(report["data_window"], serde_json::json!([-2, 3, -1, 4]));
    assert_eq!(report["width"], 2);
    assert_eq!(report["height"], 2);
    assert_eq!(report["complete"], true);
    assert_eq!(report["samples"], 12);
    let bytes = std::fs::read(&output).unwrap();
    let values = bytes
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![
            0., 100., 200., 1., 101., 201., 10., 110., 210., 11., 111., 211.
        ]
    );
    let again = run(&p, &output, "Beauty", "0");
    assert_eq!(again.status.code(), Some(2));
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    for (pass, part, kind) in [
        ("Missing", "0", "exr_selection"),
        ("Beauty", "1", "exr_selection"),
    ] {
        let target = d.join(format!("{pass}-{part}"));
        let o = run(&p, &target, pass, part);
        assert_eq!(o.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&o.stderr).contains(kind));
        assert!(!target.exists());
    }
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn corrupt_pixels_refuse_publication_not_header_success() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    let p = d.join("broken.exr");
    let mut b = fixture();
    b.truncate(b.len() - 1);
    std::fs::write(&p, b).unwrap();
    let target = d.join("out");
    let out = run(&p, &target, "Beauty", "0");
    assert_eq!(out.status.code(), Some(2));
    assert!(!target.exists());
    assert!(String::from_utf8_lossy(&out.stderr).contains("exr_decode"));
    std::fs::remove_dir_all(d).unwrap();
}

fn value_offset(bytes: &[u8], name: &str, kind: &str) -> usize {
    let needle = format!("{name}\0{kind}\0").into_bytes();
    bytes
        .windows(needle.len())
        .position(|v| v == needle)
        .unwrap()
        + needle.len()
        + 4
}
#[test]
fn complete_structure_is_not_a_pixel_decode_certificate() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    let p = d.join("bad-zip.exr");
    let mut bytes = fixture();
    let at = value_offset(&bytes, "compression", "compression");
    bytes[at] = 3;
    std::fs::write(&p, bytes).unwrap();
    let h = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-header")
        .arg(&p)
        .output()
        .unwrap();
    assert!(h.status.success());
    let facts: Value = serde_json::from_slice(&h.stdout).unwrap();
    assert_eq!(facts["truncated"], false);
    let out = d.join("no-garbage.f32");
    let result = run(&p, &out, "Beauty", "0");
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("exr_decode"));
    assert!(!out.exists());
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn decreasing_physical_chunks_keep_increasing_y_offset_table() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    let p = d.join("decreasing.exr");
    let mut bytes = fixture();
    let at = value_offset(&bytes, "lineOrder", "lineOrder");
    bytes[at] = 1;
    let start = bytes.len() - 80;
    let first = bytes[start..start + 40].to_vec();
    let second = bytes[start + 40..].to_vec();
    bytes[start..start + 40].copy_from_slice(&second);
    bytes[start + 40..].copy_from_slice(&first);
    bytes[start - 16..start - 8].copy_from_slice(&((start + 40) as u64).to_le_bytes());
    bytes[start - 8..start].copy_from_slice(&(start as u64).to_le_bytes());
    std::fs::write(&p, bytes).unwrap();
    let out = d.join("pixels.f32");
    let result = run(&p, &out, "Beauty", "0");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let values = std::fs::read(&out)
        .unwrap()
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![
            0., 100., 200., 1., 101., 201., 10., 110., 210., 11., 111., 211.
        ]
    );
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn uint_and_subsampled_channels_are_typed_refusals() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    for sub in [false, true] {
        let mut bytes = fixture();
        let name = b"Beauty.B\0";
        let at = bytes.windows(name.len()).position(|v| v == name).unwrap() + name.len();
        if sub {
            bytes[at + 8..at + 12].copy_from_slice(&2i32.to_le_bytes());
        } else {
            bytes[at..at + 4].copy_from_slice(&0i32.to_le_bytes());
        }
        let p = d.join(format!("unsupported-{sub}.exr"));
        std::fs::write(&p, bytes).unwrap();
        let out = d.join(format!("out-{sub}"));
        let result = run(&p, &out, "Beauty", "0");
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("exr_unsupported"));
        assert!(!out.exists());
    }
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn f32_signaling_nan_payload_and_signed_zero_are_preserved() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    let mut bytes = fixture();
    let start = bytes.len() - 80;
    bytes[start + 8..start + 12].copy_from_slice(&0x80000000u32.to_le_bytes());
    bytes[start + 12..start + 16].copy_from_slice(&0x7fa12345u32.to_le_bytes());
    let p = d.join("nonfinite.exr");
    std::fs::write(&p, bytes).unwrap();
    let out = d.join("pixels.f32");
    let result = run(&p, &out, "Beauty", "0");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let b = std::fs::read(&out).unwrap();
    assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 0x80000000);
    assert_eq!(
        u32::from_le_bytes(b[12..16].try_into().unwrap()),
        0x7fa12345
    );
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn half_finite_subnormal_infinity_and_nan_widening() {
    let d = support::scratch::scratch_dir(&std::env::temp_dir());
    let source = fixture();
    let end = source.len() - 96;
    let mut bytes = source[..end].to_vec();
    for name in ["Beauty.B", "Beauty.G", "Beauty.R", "Mist.Z"] {
        let n = format!("{name}\0").into_bytes();
        let at = bytes.windows(n.len()).position(|v| v == n).unwrap() + n.len();
        bytes[at..at + 4].copy_from_slice(&1i32.to_le_bytes());
    }
    let start = bytes.len() + 16;
    bytes.extend((start as u64).to_le_bytes());
    bytes.extend(((start + 24) as u64).to_le_bytes());
    for (y, values) in [
        (
            3i32,
            [
                0x3c00u16, 0xc000, 0x0000, 0x8000, 0x7c00, 0xfc00, 0x7e55, 0x7d55,
            ],
        ),
        (4, [0x0001, 0x7bff, 0x7e55, 0x7d55, 0x3800, 0xb800, 0, 0]),
    ] {
        bytes.extend(y.to_le_bytes());
        bytes.extend(16i32.to_le_bytes());
        for v in values {
            bytes.extend(v.to_le_bytes());
        }
    }
    let p = d.join("half.exr");
    std::fs::write(&p, bytes).unwrap();
    let out = d.join("pixels.f32");
    let result = run(&p, &out, "Beauty", "0");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let raw = std::fs::read(&out).unwrap();
    let bits = raw
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        &bits[..6],
        &[
            0x3f800000, 0, 0x7f800000, 0xc0000000, 0x80000000, 0xff800000
        ]
    );
    assert_eq!(bits[6], 0x33800000);
    assert!(f32::from_bits(bits[7]).is_nan());
    assert_eq!(bits[8], 0x3f000000);
    assert_eq!(bits[9], 0x477fe000);
    assert!(f32::from_bits(bits[10]).is_nan());
    assert_eq!(bits[11], 0xbf000000);
    std::fs::remove_dir_all(d).unwrap();
}
