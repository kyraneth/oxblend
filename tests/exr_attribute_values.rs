//! Hand-authored EXR header bytes, no external images, pixels or library writer.
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "oxblend-exr-values-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn attr(out: &mut Vec<u8>, name: &str, kind: &str, bytes: &[u8]) {
    out.extend(name.as_bytes());
    out.push(0);
    out.extend(kind.as_bytes());
    out.push(0);
    out.extend(i32::try_from(bytes.len()).unwrap().to_le_bytes());
    out.extend(bytes);
}
fn base() -> Vec<u8> {
    let mut b = 20000630u32.to_le_bytes().to_vec();
    b.extend(2u32.to_le_bytes());
    let mut channels = b"R\0".to_vec();
    channels.extend(2i32.to_le_bytes());
    channels.extend([0; 4]);
    channels.extend(1i32.to_le_bytes());
    channels.extend(1i32.to_le_bytes());
    channels.push(0);
    attr(&mut b, "channels", "chlist", &channels);
    for (name, kind, value) in [
        ("compression", "compression", vec![3]),
        ("dataWindow", "box2i", vec![0; 16]),
        ("displayWindow", "box2i", vec![0; 16]),
        ("lineOrder", "lineOrder", vec![0]),
        ("pixelAspectRatio", "float", 1f32.to_le_bytes().to_vec()),
        ("screenWindowCenter", "v2f", vec![0; 8]),
        ("screenWindowWidth", "float", 1f32.to_le_bytes().to_vec()),
    ] {
        attr(&mut b, name, kind, &value);
    }
    b
}
fn cli(work: &Work, mut b: Vec<u8>) -> (std::process::Output, serde_json::Value) {
    b.push(0);
    let p = work.0.join("header.exr");
    std::fs::write(&p, b).unwrap();
    let r = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-header")
        .arg(&p)
        .output()
        .unwrap();
    let j = serde_json::from_slice(if r.stdout.is_empty() {
        &r.stderr
    } else {
        &r.stdout
    })
    .unwrap();
    (r, j)
}
fn value<'a>(j: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    &j["parts"][0]["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == name)
        .unwrap()["decoded_value"]
}
fn floats(ns: &[f32]) -> Vec<u8> {
    ns.iter().flat_map(|n| n.to_le_bytes()).collect()
}
fn ints(ns: &[i32]) -> Vec<u8> {
    ns.iter().flat_map(|n| n.to_le_bytes()).collect()
}
#[test]
fn typed_values_and_blender_strings_arrive_through_real_cli() {
    let w = Work::new();
    let mut b = base();
    for (name, bytes) in [
        ("File", b"//synthetic.blend".as_slice()),
        ("Date", b"2026:10:16 00:00:00".as_slice()),
        ("RenderTime", b"00:01.25".as_slice()),
        ("cryptomatte/a/manifest", b"{corrupt json".as_slice()),
        ("bad_utf8", b"a\xff\0b".as_slice()),
    ] {
        attr(&mut b, name, "string", bytes);
    }
    attr(&mut b, "Frame", "int", &101i32.to_le_bytes());
    attr(&mut b, "f", "float", &1.25f32.to_le_bytes());
    attr(&mut b, "d", "double", &(-2.5f64).to_le_bytes());
    for (name, kind, bytes) in [
        ("vi2", "v2i", ints(&[-3, 4])),
        ("vf2", "v2f", floats(&[1., 2.])),
        ("vi3", "v3i", ints(&[1, -2, 3])),
        ("vf3", "v3f", floats(&[1., 2., 3.])),
        ("bi", "box2i", ints(&[-1, -2, 3, 4])),
        ("bf", "box2f", floats(&[-1., -2., 3., 4.])),
        (
            "m3",
            "m33f",
            floats(&(0..9).map(|n| n as f32).collect::<Vec<_>>()),
        ),
        (
            "m4",
            "m44f",
            floats(&(0..16).map(|n| n as f32).collect::<Vec<_>>()),
        ),
    ] {
        attr(&mut b, name, kind, &bytes);
    }
    let (r, j) = cli(&w, b);
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["pixel_integrity_verified"], false);
    assert_eq!(value(&j, "Frame")["value"], 101);
    assert_eq!(value(&j, "f")["value"], 1.25);
    assert_eq!(value(&j, "d")["value"], -2.5);
    assert_eq!(value(&j, "File")["value"]["text"], "//synthetic.blend");
    assert_eq!(
        value(&j, "cryptomatte/a/manifest")["value"]["text"],
        "{corrupt json"
    );
    assert_eq!(value(&j, "bad_utf8")["value"]["utf8_valid"], false);
    assert!(value(&j, "bad_utf8")["value"]["text"].is_null());
    assert_eq!(value(&j, "bad_utf8")["value"]["hex"], "61ff0062");
    assert_eq!(value(&j, "vi2")["value"], serde_json::json!([-3, 4]));
    assert_eq!(value(&j, "vf2")["value"], serde_json::json!([1., 2.]));
    assert_eq!(value(&j, "vi3")["value"], serde_json::json!([1, -2, 3]));
    assert_eq!(value(&j, "vf3")["value"], serde_json::json!([1., 2., 3.]));
    assert_eq!(
        value(&j, "bi")["value"],
        serde_json::json!({"min":[-1,-2],"max":[3,4]})
    );
    assert_eq!(
        value(&j, "bf")["value"],
        serde_json::json!({"min":[-1.,-2.],"max":[3.,4.]})
    );
    assert_eq!(value(&j, "m3")["value"][2][1], 7.);
    assert_eq!(value(&j, "m4")["value"][3][3], 15.);
    assert_eq!(
        value(&j, "compression")["value"],
        serde_json::json!({"code":3,"name":"zip"})
    );
    assert_eq!(value(&j, "channels")["value"][0]["name"], "R");
    for attribute in j["parts"][0]["attributes"].as_array().unwrap() {
        assert_eq!(attribute["decoded_value"]["status"], "decoded");
        assert_eq!(attribute["decoded_value"]["decoded"], true);
    }
}
#[test]
fn value_refusals_are_local_and_do_not_consume_the_next_attribute() {
    let w = Work::new();
    let mut b = base();
    attr(&mut b, "bad_int", "int", &[1, 2, 3]);
    attr(&mut b, "unknown", "future", &[1, 2, 3, 4]);
    attr(&mut b, "bad_float", "float", &f32::NAN.to_le_bytes());
    attr(&mut b, "enum", "lineOrder", &[255]);
    attr(&mut b, "after", "int", &42i32.to_le_bytes());
    let (r, j) = cli(&w, b);
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["header_complete"], true);
    assert_eq!(value(&j, "bad_int")["kind"], "attribute_length");
    assert_eq!(value(&j, "unknown")["kind"], "unknown_attribute_type");
    assert_eq!(value(&j, "unknown")["raw_hex"], "01020304");
    assert_eq!(value(&j, "bad_float")["kind"], "nonfinite_attribute_float");
    assert_eq!(value(&j, "enum")["kind"], "attribute_value");
    assert_eq!(value(&j, "after")["value"], 42);
    for name in ["bad_int", "unknown", "bad_float", "enum"] {
        assert_eq!(value(&j, name)["status"], "refused");
        assert_eq!(value(&j, name)["decoded"], false);
    }
    let unknown = j["parts"][0]["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "unknown")
        .unwrap();
    assert_eq!(unknown["length"], 4);
    assert!(unknown["offset"].is_u64());
    assert!(unknown["decoded_value"].get("length").is_none());
    assert!(unknown["decoded_value"].get("offset").is_none());
}
#[test]
fn truncated_or_oversized_declared_attributes_are_header_structure_errors() {
    let w = Work::new();
    for size in [8i32, i32::MAX] {
        let mut b = base();
        b.extend(b"broken\0int\0");
        b.extend(size.to_le_bytes());
        b.extend([1, 2]);
        let (r, j) = cli(&w, b);
        assert_eq!(r.status.code(), Some(2), "{j}");
        assert_eq!(j["error"]["kind"], "exr_header");
        let message = j["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("broken") && message.contains("expected") && message.contains("found"),
            "{message}"
        );
    }
}
#[test]
fn wrong_magic_and_channel_extent_errors_explain_expected_and_found() {
    let w = Work::new();
    let (out, j) = cli(&w, b"BLENDER-v405".to_vec());
    assert_eq!(out.status.code(), Some(2));
    let message = j["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("expected OpenEXR magic 20000630") && message.contains("found"),
        "{message}"
    );
    let mut b = 20000630u32.to_le_bytes().to_vec();
    b.extend(2u32.to_le_bytes());
    attr(&mut b, "channels", "chlist", b"R\0\0\0");
    let (out, j) = cli(&w, b);
    assert_eq!(out.status.code(), Some(2));
    let message = j["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("channel 'R'") && message.contains("expected 16 metadata bytes, found 2"),
        "{message}"
    );
}

// Synthetic values follow the public OpenEXR File Layout attribute table.
fn uints(ns: &[u32]) -> Vec<u8> {
    ns.iter().flat_map(|n| n.to_le_bytes()).collect()
}
#[test]
fn standard_metadata_types_arrive_without_interpreting_pixels_or_timecode() {
    let w = Work::new();
    let mut b = base();
    attr(
        &mut b,
        "time",
        "timecode",
        &uints(&[0xfedcba98, 0x87654321]),
    );
    attr(&mut b, "key", "keycode", &ints(&[1, 2, -3, 4, 5, 6, 7]));
    let mut fraction = (-24i32).to_le_bytes().to_vec();
    fraction.extend(u32::MAX.to_le_bytes());
    attr(&mut b, "fraction", "rational", &fraction);
    attr(&mut b, "zero_denominator", "rational", &uints(&[1, 0]));
    let mut tile = uints(&[64, 32]);
    tile.push(0x12);
    attr(&mut b, "tile", "tiledesc", &tile);
    let dimensions = uints(&[2, 1]);
    let mut preview = dimensions.clone();
    preview.extend([255, 0, 0, 255, 0, 255, 0, 255]);
    attr(&mut b, "preview_full", "preview", &preview);
    attr(
        &mut b,
        "chroma",
        "chromaticities",
        &floats(&[1., 2., 3., 4., 5., 6., 7., 8.]),
    );
    attr(&mut b, "environment", "envmap", &[1]);
    let mut strings = Vec::new();
    for s in [b"left".as_slice(), b"".as_slice(), b"\xff\0".as_slice()] {
        strings.extend((s.len() as i32).to_le_bytes());
        strings.extend(s);
    }
    attr(&mut b, "views", "stringvector", &strings);
    attr(&mut b, "empty_views", "stringvector", &[]);
    let mut blob = uints(&[3]);
    blob.extend(b"abc\0\xff");
    attr(&mut b, "blob", "bytes", &blob);
    attr(&mut b, "empty_blob", "bytes", &uints(&[0]));
    let (r, j) = cli(&w, b);
    assert_eq!(r.status.code(), Some(0), "{j}");
    assert_eq!(j["header_complete"], true);
    assert_eq!(j["pixel_integrity_verified"], false);
    for attribute in j["parts"][0]["attributes"].as_array().unwrap() {
        assert_eq!(attribute["decoded_value"]["decoded"], true, "{attribute}");
    }
    assert_eq!(
        value(&j, "time")["value"],
        serde_json::json!({"timeAndFlags":0xfedcba98u32,"userData":0x87654321u32})
    );
    assert_eq!(
        value(&j, "key")["value"],
        serde_json::json!({"filmMfcCode":1,"filmType":2,"prefix":-3,"count":4,"perfOffset":5,"perfsPerFrame":6,"perfsPerCount":7})
    );
    assert_eq!(
        value(&j, "fraction")["value"],
        serde_json::json!({"numerator":-24,"denominator":u32::MAX})
    );
    assert_eq!(value(&j, "zero_denominator")["value"]["denominator"], 0);
    assert_eq!(
        value(&j, "tile")["value"],
        serde_json::json!({"xSize":64,"ySize":32,"mode":18,"levelMode":{"code":2,"name":"ripmap_levels"},"roundingMode":{"code":1,"name":"round_up"}})
    );
    let expected = serde_json::json!({"width":2,"height":1});
    assert_eq!(value(&j, "preview_full")["value"], expected);
    assert_eq!(
        value(&j, "chroma")["value"],
        serde_json::json!({"redX":1.,"redY":2.,"greenX":3.,"greenY":4.,"blueX":5.,"blueY":6.,"whiteX":7.,"whiteY":8.})
    );
    assert_eq!(
        value(&j, "environment")["value"],
        serde_json::json!({"code":1,"name":"cube"})
    );
    assert_eq!(value(&j, "views")["value"][0]["text"], "left");
    assert_eq!(value(&j, "views")["value"][1]["text"], "");
    assert_eq!(value(&j, "views")["value"][2]["utf8_valid"], false);
    assert_eq!(value(&j, "views")["value"][2]["hex"], "ff00");
    assert_eq!(value(&j, "empty_views")["value"], serde_json::json!([]));
    assert_eq!(value(&j, "blob")["value"]["typeHint"]["text"], "abc");
    assert_eq!(value(&j, "blob")["value"]["hex"], "00ff");
    assert_eq!(value(&j, "empty_blob")["value"]["hex"], "");
}

#[test]
fn preview_declared_size_is_checked_without_loading_embedded_samples() {
    let w = Work::new();
    let mut b = base();
    attr(&mut b, "short_preview", "preview", &uints(&[2, 1]));
    attr(
        &mut b,
        "overflow_preview",
        "preview",
        &uints(&[u32::MAX, u32::MAX]),
    );
    attr(&mut b, "following", "int", &42i32.to_le_bytes());
    let (r, j) = cli(&w, b);
    assert!(r.status.success());
    for name in ["short_preview", "overflow_preview"] {
        assert_eq!(value(&j, name)["decoded"], false);
        assert_eq!(value(&j, name)["kind"], "attribute_length");
        assert!(value(&j, name)["raw_prefix_hex"].is_string());
    }
    assert_eq!(value(&j, "short_preview")["expected_length"], 16);
    assert_eq!(
        value(&j, "overflow_preview")["expected_length"],
        serde_json::Value::Null
    );
    assert_eq!(value(&j, "following")["value"], 42);
}
#[test]
fn channel_aliases_preserve_original_fields_for_every_pixel_type() {
    let w = Work::new();
    let mut b = base();
    let mut channels = Vec::new();
    for pixel_type in 0i32..=2 {
        channels.extend(format!("c{pixel_type}\0").as_bytes());
        channels.extend(pixel_type.to_le_bytes());
        channels.extend([u8::from(pixel_type == 1), 0, 0, 0]);
        channels.extend(2i32.to_le_bytes());
        channels.extend(3i32.to_le_bytes());
    }
    channels.push(0);
    attr(&mut b, "other_channels", "chlist", &channels);
    let (r, j) = cli(&w, b);
    assert_eq!(r.status.code(), Some(0), "{j}");
    for (i, name) in ["uint", "half", "float"].iter().enumerate() {
        let c = &value(&j, "other_channels")["value"][i];
        assert_eq!(c["pixel_type"], i);
        assert_eq!(c["pixel_type_name"], *name);
        assert_eq!(c["linear"], usize::from(i == 1));
        assert_eq!(c["pLinear"], i == 1);
        assert_eq!(c["reserved"], serde_json::json!([0, 0, 0]));
        assert_eq!(c["sampling"], serde_json::json!([2, 3]));
        assert_eq!(c["xSampling"], 2);
        assert_eq!(c["ySampling"], 3);
    }
}

#[test]
fn standard_metadata_length_refusals_remain_local() {
    let w = Work::new();
    for (kind, length) in [
        ("timecode", 8),
        ("keycode", 28),
        ("rational", 8),
        ("tiledesc", 9),
        ("chromaticities", 32),
        ("envmap", 1),
    ] {
        for wrong in [length - 1, length + 1] {
            let mut b = base();
            attr(&mut b, "bad", kind, &vec![0; wrong]);
            attr(&mut b, "after", "int", &42i32.to_le_bytes());
            let (r, j) = cli(&w, b);
            assert_eq!(r.status.code(), Some(0), "{kind}: {j}");
            assert_eq!(value(&j, "bad")["kind"], "attribute_length", "{kind}: {j}");
            assert_eq!(value(&j, "bad")["decoded"], false);
            assert_eq!(value(&j, "after")["value"], 42);
        }
    }
    for (kind, payload, reason) in [
        ("stringvector", vec![0; 3], "attribute_length"),
        ("stringvector", ints(&[-1]), "attribute_value"),
        ("stringvector", ints(&[1]), "attribute_length"),
        ("bytes", vec![0; 3], "attribute_length"),
        ("bytes", uints(&[u32::MAX]), "attribute_length"),
        ("envmap", vec![2], "attribute_value"),
        (
            "tiledesc",
            vec![0, 0, 0, 0, 0, 0, 0, 0, 3],
            "attribute_value",
        ),
        (
            "tiledesc",
            vec![0, 0, 0, 0, 0, 0, 0, 0, 32],
            "attribute_value",
        ),
        (
            "chromaticities",
            floats(&[f32::INFINITY; 8]),
            "nonfinite_attribute_float",
        ),
        ("chlist", b"R\0\0".to_vec(), "attribute_value"),
    ] {
        let mut b = base();
        attr(&mut b, "bad", kind, &payload);
        attr(&mut b, "after", "int", &42i32.to_le_bytes());
        let (r, j) = cli(&w, b);
        assert_eq!(r.status.code(), Some(0), "{kind}: {j}");
        assert_eq!(value(&j, "bad")["kind"], reason, "{kind}: {j}");
        assert_eq!(value(&j, "bad")["decoded"], false);
        assert_eq!(value(&j, "after")["value"], 42);
    }
}
