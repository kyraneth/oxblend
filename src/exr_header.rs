use crate::edit_output::io;
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek};
mod values;
use std::path::Path;
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Channel {
    pub name: Vec<u8>,
    pub pixel_type: i32,
    pub linear: u8,
    pub reserved: [u8; 3],
    pub sampling: [i32; 2],
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Attribute {
    pub name: Vec<u8>,
    pub kind: Vec<u8>,
    pub offset: u64,
    pub length: u32,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct Part {
    pub channels: Vec<Channel>,
    pub attributes: Vec<Attribute>,
    pub values: BTreeMap<Vec<u8>, Vec<u8>>,
}
#[derive(Debug)]
#[non_exhaustive]
pub struct ExrHeader {
    pub version: u32,
    pub parts: Vec<Part>,
    pub extent: u64,
}
fn err(s: &str) -> Error {
    Error::new("exr_header", s)
}
fn i32le(b: &[u8]) -> Result<i32> {
    Ok(i32::from_le_bytes(b.try_into().map_err(|_| {
        err(&format!(
            "expected a 4-byte integer, found {} bytes",
            b.len()
        ))
    })?))
}
fn cstring<R: Read>(r: &mut R, max: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut b = [0];
        r.read_exact(&mut b).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                err(&format!(
                    "expected NUL-terminated name, found EOF after {} name bytes",
                    out.len()
                ))
            } else {
                io(e)
            }
        })?;
        if b[0] == 0 {
            return Ok(out);
        }
        if out.len() == max {
            return Err(err(&format!(
                "expected name of at most {max} bytes, found additional non-NUL byte"
            )));
        }
        out.push(b[0]);
    }
}
fn read_channels(value: &[u8], max: usize) -> Result<Vec<Channel>> {
    let mut cursor = std::io::Cursor::new(value);
    let mut names = BTreeSet::new();
    let mut channels = Vec::new();
    loop {
        let name = cstring(&mut cursor, max)?;
        if name.is_empty() {
            break;
        }
        if !names.insert(name.clone()) {
            return Err(err(&format!(
                "expected unique channel names, found duplicate '{}'",
                crate::value::display_bytes(&name)
            )));
        }
        let mut record = [0; 16];
        let remaining = value.len() as u64 - cursor.position();
        cursor.read_exact(&mut record).map_err(|_| {
            err(&format!(
                "channel '{}': expected 16 metadata bytes, found {remaining}",
                crate::value::display_bytes(&name)
            ))
        })?;
        let pixel_type = i32le(&record[..4])?;
        let sampling = [i32le(&record[8..12])?, i32le(&record[12..])?];
        if !matches!(pixel_type, 0..=2) || record[4] > 1 || sampling.iter().any(|n| *n <= 0) {
            return Err(err(&format!(
                "channel '{}': expected pixel type 0..2, linear 0..1 and positive sampling; found type {pixel_type}, linear {}, sampling {}x{}",
                crate::value::display_bytes(&name),
                record[4],
                sampling[0],
                sampling[1]
            )));
        }
        channels.push(Channel {
            name,
            pixel_type,
            linear: record[4],
            reserved: record[5..8].try_into().unwrap(),
            sampling,
        });
    }
    if cursor.position() != value.len() as u64 {
        return Err(err(&format!(
            "expected chlist to end after its NUL terminator at byte {}, found {} trailing bytes",
            cursor.position(),
            value.len() as u64 - cursor.position()
        )));
    }
    Ok(channels)
}
pub fn read(path: &Path) -> Result<ExrHeader> {
    crate::diagnostics::opened(path, || read_inner(path))
}
fn read_inner(path: &Path) -> Result<ExrHeader> {
    let mut f = File::open(path).map_err(io)?;
    let before = f.metadata().map_err(io)?;
    let length = before.len();
    if length < 8 {
        return Err(err(&format!(
            "expected 8-byte EXR prefix, found {length} bytes"
        )));
    }
    let mut prefix = [0; 8];
    f.read_exact(&mut prefix).map_err(io)?;
    if prefix[..4] != [0x76, 0x2f, 0x31, 0x01] {
        return Err(err(&format!(
            "expected OpenEXR magic 20000630 (bytes 762f3101), found {} (bytes {})",
            u32::from_le_bytes(prefix[..4].try_into().unwrap()),
            crate::value::hex(&prefix[..4])
        )));
    }
    let version = u32::from_le_bytes(prefix[4..].try_into().unwrap());
    if version & 0xff != 2 || version & !0x1e02 != 0 {
        return Err(Error::new(
            "exr_unsupported",
            format!(
                "expected EXR version 2 with supported flags 0x1e00, found version/flags 0x{version:08x}"
            ),
        ));
    }
    let multipart = version & 0x1000 != 0;
    let max = if version & 0x400 != 0 { 255 } else { 31 };
    let mut parts = Vec::new();
    loop {
        let mut part = Part {
            channels: Vec::new(),
            attributes: Vec::new(),
            values: BTreeMap::new(),
        };
        let mut names = BTreeSet::new();
        loop {
            let name = cstring(&mut f, max)?;
            if name.is_empty() {
                break;
            }
            if !names.insert(name.clone()) {
                return Err(err(&format!(
                    "expected unique attribute names, found duplicate '{}'",
                    crate::value::display_bytes(&name)
                )));
            }
            let kind = cstring(&mut f, max)?;
            if kind.is_empty() {
                return Err(err(&format!(
                    "attribute '{}': expected nonempty type, found empty string",
                    crate::value::display_bytes(&name)
                )));
            }
            let mut n = [0; 4];
            f.read_exact(&mut n).map_err(io)?;
            let n = i32le(&n)?;
            if n < 0 {
                return Err(err(&format!(
                    "attribute '{}': expected nonnegative size, found {n}",
                    crate::value::display_bytes(&name)
                )));
            }
            let offset = f.stream_position().map_err(io)?;
            let end = offset
                .checked_add(n as u64)
                .ok_or_else(|| err(&format!("expected representable attribute end, found offset {offset} + size {n} overflow")))?;
            if end > length {
                return Err(err(&format!(
                    "attribute '{}': expected {n} bytes at offset {offset}, found only {} bytes before EOF",
                    crate::value::display_bytes(&name),
                    length.saturating_sub(offset)
                )));
            }
            // Retain exactly this payload, including application metadata. No
            // decoder can read into the following attribute or pixel chunks.
            let mut value = vec![0; n as usize];
            f.read_exact(&mut value).map_err(io)?;
            if name == b"channels" {
                if kind != b"chlist" {
                    return Err(err(&format!(
                        "attribute channels: expected type chlist, found '{}'",
                        crate::value::display_bytes(&kind)
                    )));
                }
                part.channels = read_channels(&value, max)?;
            }
            part.values.insert(name.clone(), value);
            part.attributes.push(Attribute {
                name,
                kind,
                offset,
                length: n as u32,
            });
        }
        if part.attributes.is_empty() {
            if multipart && !parts.is_empty() {
                break;
            }
            return Err(err("expected a nonempty EXR header, found no attributes"));
        }
        for (n, k, size) in [
            ("channels", "chlist", None),
            ("compression", "compression", Some(1)),
            ("dataWindow", "box2i", Some(16)),
            ("displayWindow", "box2i", Some(16)),
            ("lineOrder", "lineOrder", Some(1)),
            ("pixelAspectRatio", "float", Some(4)),
            ("screenWindowCenter", "v2f", Some(8)),
            ("screenWindowWidth", "float", Some(4)),
        ] {
            let a = part
                .attributes
                .iter()
                .find(|a| a.name == n.as_bytes())
                .ok_or_else(|| {
                    err(&format!(
                        "expected required attribute '{n}' of type {k}, found none"
                    ))
                })?;
            if a.kind != k.as_bytes() || size.is_some_and(|s| a.length != s) {
                let expected_size = size.map_or_else(|| "variable".to_owned(), |s| s.to_string());
                return Err(err(&format!(
                    "attribute '{n}': expected type {k}, size {expected_size}; found type '{}', size {}",
                    crate::value::display_bytes(&a.kind),
                    a.length
                )));
            }
        }
        parts.push(part);
        if !multipart {
            break;
        }
    }
    let extent = f.stream_position().map_err(io)?;
    let after = f.metadata().map_err(io)?;
    if before.len() != after.len()
        || before.modified().map_err(io)? != after.modified().map_err(io)?
    {
        return Err(Error::new(
            "input_changed",
            "EXR changed during header read",
        ));
    }
    Ok(ExrHeader {
        version,
        parts,
        extent,
    })
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Pass {
    pub name: String,
    pub socket_type: &'static str,
    pub components: Vec<String>,
    /// Original EXR group identity, independent of the displayed output name.
    /// A synthetic Alpha output refers to the same group as its Image output.
    pub source_pass: String,
}
impl ExrHeader {
    pub fn passes_for_saved_version(&self, saved_version: u32) -> Result<Vec<Pass>> {
        if !matches!(saved_version, 405 | 502) {
            return Err(Error::new(
                "exr_unsupported",
                "unqualified saved pass metadata version",
            ));
        }
        let mut passes = self.passes()?;
        if saved_version == 405 {
            let priority = |name: &str| match name {
                "Depth" => 0,
                "Mist" => 1,
                "Normal" => 2,
                "UV" => 3,
                "Position" => 4,
                _ => 5,
            };
            passes.sort_by(|a, b| {
                priority(&a.name)
                    .cmp(&priority(&b.name))
                    .then_with(|| a.name.cmp(&b.name))
            });
            for pass in &mut passes {
                if pass.name == "Alpha" {
                    pass.source_pass = "Combined".to_owned();
                }
            }
        }
        Ok(passes)
    }

    pub fn passes(&self) -> Result<Vec<Pass>> {
        let unsupported = |message: &str| Error::new("exr_unsupported", message);
        if self.version & 0xff != 2 || self.version & !0x1e02 != 0 {
            return Err(unsupported("unknown version/flags"));
        }
        if self.parts.len() != 1 || self.version & 0x1000 != 0 {
            return Err(unsupported("compositor requires a single EXR part"));
        }
        let part = &self.parts[0];
        if self.version & 0x800 != 0
            || part
                .values
                .get(b"type".as_slice())
                .is_some_and(|t| !matches!(t.as_slice(), b"scanlineimage" | b"tiledimage"))
        {
            return Err(unsupported("deep/non-image EXR compositor input"));
        }
        if part.values.contains_key(b"multiView".as_slice())
            || part.values.contains_key(b"view".as_slice())
        {
            return Err(unsupported("EXR view selection is not supported"));
        }
        if part.channels.is_empty() {
            return Err(unsupported("no EXR pass channels"));
        }

        // A separate lookup indexes the ordered groups; neither pass order nor
        // component order comes from the sorted map. Split from the right so
        // dots in a layer name retain their identity.
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        let mut indices = BTreeMap::new();
        let mut layer = None;
        for channel in &part.channels {
            let name = std::str::from_utf8(&channel.name)
                .map_err(|_| unsupported("EXR compositor channel name must be UTF8"))?;
            let (prefix, component) = name
                .rsplit_once('.')
                .ok_or_else(|| unsupported("bare EXR channels need an explicit pass selection"))?;
            let (channel_layer, pass) = prefix.rsplit_once('.').unwrap_or(("", prefix));
            if pass.is_empty() || component.is_empty() || prefix.starts_with('.') {
                return Err(unsupported("empty EXR layer/pass/component identity"));
            }
            if layer.is_some_and(|selected| selected != channel_layer) {
                return Err(unsupported(
                    "multiple EXR layers need an explicit selection",
                ));
            }
            layer = Some(channel_layer);
            if channel.sampling != [1, 1] {
                return Err(unsupported("subsampled compositor pass channels"));
            }
            let index = *indices.entry(pass.to_owned()).or_insert_with(|| {
                groups.push((pass.to_owned(), Vec::new()));
                groups.len() - 1
            });
            if groups[index].1.iter().any(|c| c == component) {
                return Err(err("duplicate pass component"));
            }
            groups[index].1.push(component.to_owned());
        }
        let explicit_alpha = groups.iter().any(|(name, _)| name == "Alpha");
        let mut names = BTreeSet::new();
        let mut passes = Vec::new();
        for (source_pass, components) in groups {
            let identities = components
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            // Shape-based generic passes are deliberately qualified only for
            // these observed component identities, not merely channel counts.
            // Blender 5.2.1 native fixtures qualify UV's A/U/V and W/X/Y/Z.
            let socket_type = if identities == BTreeSet::from(["R", "G", "B"])
                || identities == BTreeSet::from(["R", "G", "B", "A"])
                || identities == BTreeSet::from(["r", "g", "b"])
                || identities == BTreeSet::from(["r", "g", "b", "a"])
            {
                "NodeSocketColor"
            } else if identities == BTreeSet::from(["X", "Y", "Z"])
                || (source_pass == "UV" && identities == BTreeSet::from(["U", "V", "A"]))
            {
                "NodeSocketVector"
            } else if identities == BTreeSet::from(["W", "X", "Y", "Z"]) {
                "NodeSocketVector4D"
            } else if components.len() == 1 {
                "NodeSocketFloat"
            } else {
                return Err(unsupported("unqualified EXR pass component identities"));
            };
            let combined = source_pass == "Combined";
            if combined && socket_type != "NodeSocketColor" {
                return Err(unsupported(
                    "combined image pass must contain color components",
                ));
            }
            if source_pass == "Alpha" && socket_type != "NodeSocketFloat" {
                return Err(unsupported("explicit Alpha pass must be scalar"));
            }
            let synthetic_alpha = combined && identities.contains("A") && !explicit_alpha;
            let name = if source_pass == "Combined" {
                "Image".to_owned()
            } else {
                source_pass.clone()
            };
            if !names.insert(name.clone()) {
                return Err(unsupported(
                    "EXR output names collide after image adaptation",
                ));
            }
            passes.push(Pass {
                name,
                socket_type,
                components,
                source_pass: source_pass.clone(),
            });
            if synthetic_alpha {
                if !names.insert("Alpha".to_owned()) {
                    return Err(unsupported("multiple combined image passes"));
                }
                passes.push(Pass {
                    name: "Alpha".to_owned(),
                    socket_type: "NodeSocketFloat",
                    components: vec!["A".to_owned()],
                    source_pass,
                });
            }
        }
        Ok(passes)
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"version":self.version,"extent":self.extent,"header_complete":true,"pixel_integrity_verified":false,"parts":self.parts.iter().map(|p|serde_json::json!({"channels":p.channels.iter().map(|c|serde_json::json!({"name_hex":crate::value::hex(&c.name),"pixel_type":c.pixel_type,"linear":c.linear,"reserved":c.reserved,"sampling":c.sampling})).collect::<Vec<_>>(),"attributes":p.attributes.iter().map(|a|serde_json::json!({"name_hex":crate::value::hex(&a.name),"type_hex":crate::value::hex(&a.kind),"offset":a.offset,"length":a.length,"name":std::str::from_utf8(&a.name).ok(),"type":std::str::from_utf8(&a.kind).ok(),"decoded_value":p.values.get(&a.name).map(|b|values::decode(&a.kind,b,if self.version & 0x400 != 0 {255}else{31}))})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod composition_vectors {
    use super::*;

    fn header(names: &[&str]) -> ExrHeader {
        ExrHeader {
            version: 2,
            extent: 0,
            parts: vec![Part {
                channels: names
                    .iter()
                    .map(|name| Channel {
                        name: name.as_bytes().to_vec(),
                        pixel_type: 2,
                        linear: 0,
                        reserved: [0; 3],
                        sampling: [1, 1],
                    })
                    .collect(),
                attributes: Vec::new(),
                values: BTreeMap::new(),
            }],
        }
    }
    fn names(passes: &[Pass]) -> Vec<&str> {
        passes.iter().map(|p| p.name.as_str()).collect()
    }
    fn wire_header(h: &ExrHeader) -> ExrHeader {
        fn attribute(bytes: &mut Vec<u8>, name: &str, kind: &str, value: &[u8]) {
            bytes.extend(name.as_bytes());
            bytes.push(0);
            bytes.extend(kind.as_bytes());
            bytes.push(0);
            bytes.extend(i32::try_from(value.len()).unwrap().to_le_bytes());
            bytes.extend(value);
        }
        let mut bytes = 20000630u32.to_le_bytes().to_vec();
        // Format long-name flag permits the dotted-layer identity.
        bytes.extend((2u32 | 0x400).to_le_bytes());
        let mut channels = Vec::new();
        for c in &h.parts[0].channels {
            channels.extend(&c.name);
            channels.push(0);
            channels.extend(c.pixel_type.to_le_bytes());
            channels.push(c.linear);
            channels.extend(c.reserved);
            for n in c.sampling {
                channels.extend(n.to_le_bytes());
            }
        }
        channels.push(0);
        attribute(&mut bytes, "channels", "chlist", &channels);
        for (name, kind, value) in [
            ("compression", "compression", vec![0]),
            ("dataWindow", "box2i", vec![0; 16]),
            ("displayWindow", "box2i", vec![0; 16]),
            ("lineOrder", "lineOrder", vec![0]),
            ("pixelAspectRatio", "float", 1f32.to_le_bytes().to_vec()),
            ("screenWindowCenter", "v2f", vec![0; 8]),
            ("screenWindowWidth", "float", 1f32.to_le_bytes().to_vec()),
        ] {
            attribute(&mut bytes, name, kind, &value);
        }
        bytes.push(0);
        let dir = crate::test_scratch::scratch_dir(&std::env::temp_dir());
        let path = dir.join("header.exr");
        std::fs::write(&path, bytes).unwrap();
        let parsed = read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
        parsed
    }

    #[test]
    fn version_405_orders_only_present_standard_families_and_lexical_remainder() {
        let h = wire_header(&header(&[
            "CustomZ.V",
            "UV.X",
            "UV.Y",
            "UV.Z",
            "Mist.V",
            "Position.X",
            "Position.Y",
            "Position.Z",
            "CustomA.V",
            "Normal.X",
            "Normal.Y",
            "Normal.Z",
            "Depth.V",
            "Alpha.V",
        ]));
        let legacy = h.passes_for_saved_version(405).unwrap();
        assert_eq!(
            names(&legacy),
            [
                "Depth", "Mist", "Normal", "UV", "Position", "Alpha", "CustomA", "CustomZ"
            ]
        );
        assert_eq!(legacy[5].source_pass, "Combined");
        let native = h.passes_for_saved_version(502).unwrap();
        assert_eq!(
            names(&native),
            [
                "CustomZ", "UV", "Mist", "Position", "CustomA", "Normal", "Depth", "Alpha"
            ]
        );
        assert_eq!(native[7].source_pass, "Alpha");
        let missing = header(&["CustomZ.V", "Normal.X", "Normal.Y", "Normal.Z", "CustomA.V"]);
        assert_eq!(
            names(&missing.passes_for_saved_version(405).unwrap()),
            ["Normal", "CustomA", "CustomZ"]
        );
        assert_eq!(
            names(
                &header(&["CustomZ.V", "CustomA.V"])
                    .passes_for_saved_version(405)
                    .unwrap()
            ),
            ["CustomA", "CustomZ"]
        );
    }

    #[test]
    fn saved_version_alpha_policy_does_not_invent_missing_channels_or_drop_vectors() {
        for version in [405, 502] {
            let synthetic = header(&["Combined.R", "Combined.G", "Combined.B", "Combined.A"])
                .passes_for_saved_version(version)
                .unwrap();
            assert_eq!(synthetic.len(), 2);
            assert!(synthetic.iter().all(|p| p.source_pass == "Combined"));
            let rgb = header(&["Combined.R", "Combined.G", "Combined.B"])
                .passes_for_saved_version(version)
                .unwrap();
            assert_eq!(names(&rgb), ["Image"]);
            let genuine = header(&[
                "Alpha.V",
                "Combined.R",
                "Combined.G",
                "Combined.B",
                "Combined.A",
            ])
            .passes_for_saved_version(version)
            .unwrap();
            assert_eq!(genuine.len(), 2);
            assert_eq!(
                genuine
                    .iter()
                    .find(|p| p.name == "Alpha")
                    .unwrap()
                    .source_pass,
                if version == 405 { "Combined" } else { "Alpha" }
            );
            let vectors = header(&[
                "Four.W", "Four.X", "Four.Y", "Four.Z", "Normal.X", "Normal.Y", "Normal.Z",
            ])
            .passes_for_saved_version(version)
            .unwrap();
            assert_eq!(
                vectors
                    .iter()
                    .find(|p| p.name == "Four")
                    .unwrap()
                    .socket_type,
                "NodeSocketVector4D"
            );
            assert_eq!(
                vectors
                    .iter()
                    .find(|p| p.name == "Normal")
                    .unwrap()
                    .socket_type,
                "NodeSocketVector"
            );
            for channels in [
                vec!["Alpha.X", "Alpha.Y", "Alpha.Z"],
                vec!["Unknown.C1", "Unknown.C2"],
                vec!["Combined.V"],
            ] {
                assert!(header(&channels).passes_for_saved_version(version).is_err());
            }
        }
        assert_eq!(
            header(&["Custom.V"])
                .passes_for_saved_version(503)
                .unwrap_err()
                .kind,
            "exr_unsupported"
        );
    }

    #[test]
    fn raw_header_order_drives_pass_and_component_order() {
        let h = wire_header(&header(&[
            "Layer.with.dots.Combined.B",
            "Layer.with.dots.Depth.Z",
            "Layer.with.dots.Combined.R",
            "Layer.with.dots.Combined.G",
            "Layer.with.dots.Combined.A",
        ]));
        let p = h.passes().unwrap();
        assert_eq!(names(&p), ["Image", "Alpha", "Depth"]);
        assert_eq!(p[0].components, ["B", "R", "G", "A"]);
        assert_eq!(p[0].source_pass, "Combined");
        assert_eq!(p[1].source_pass, p[0].source_pass);
        assert_eq!(p[2].components, ["Z"]);
        assert_eq!(p[2].socket_type, "NodeSocketFloat");
    }
    #[test]
    fn depth_before_combined_and_custom_stays_first() {
        let h = wire_header(&header(&[
            "Layer.Depth.Z",
            "Layer.Combined.R",
            "Layer.Combined.G",
            "Layer.Combined.B",
            "Layer.Combined.A",
            "Layer.Custom.X",
        ]));
        assert_eq!(
            names(&h.passes().unwrap()),
            ["Depth", "Image", "Alpha", "Custom"]
        );
    }
    #[test]
    fn custom_and_standard_groups_are_not_sorted_or_cached() {
        for (channels, expected) in [
            (
                vec![
                    "L.CustomZ.Z",
                    "L.Normal.X",
                    "L.Normal.Y",
                    "L.Normal.Z",
                    "L.CustomA.Z",
                ],
                vec!["CustomZ", "Normal", "CustomA"],
            ),
            (
                vec![
                    "L.CustomA.Z",
                    "L.CustomZ.Z",
                    "L.Normal.X",
                    "L.Normal.Y",
                    "L.Normal.Z",
                ],
                vec!["CustomA", "CustomZ", "Normal"],
            ),
        ] {
            assert_eq!(
                names(&wire_header(&header(&channels)).passes().unwrap()),
                expected
            );
        }
    }
    #[test]
    fn combined_without_alpha_component_does_not_invent_an_alpha_output() {
        let p = header(&["L.Combined.R", "L.Combined.G", "L.Combined.B"])
            .passes()
            .unwrap();
        assert_eq!(names(&p), ["Image"]);
        assert_eq!(p[0].source_pass, "Combined");
    }
    #[test]
    fn explicit_alpha_keeps_its_position_without_a_synthetic_duplicate() {
        let p = header(&[
            "L.Depth.Z",
            "L.Alpha.A",
            "L.Combined.R",
            "L.Combined.G",
            "L.Combined.B",
            "L.Combined.A",
        ])
        .passes()
        .unwrap();
        assert_eq!(names(&p), ["Depth", "Alpha", "Image"]);
        assert_eq!(p[1].source_pass, "Alpha");
    }
    #[test]
    fn component_identity_not_count_selects_socket_type() {
        let p = header(&[
            "L.Color.B",
            "L.Color.R",
            "L.Color.G",
            "L.Normal.Z",
            "L.Normal.X",
            "L.Normal.Y",
            "L.Vector.W",
            "L.Vector.X",
            "L.Vector.Y",
            "L.Vector.Z",
            "L.UV.A",
            "L.UV.U",
            "L.UV.V",
            "L.CryptoObject00.a",
            "L.CryptoObject00.b",
            "L.CryptoObject00.g",
            "L.CryptoObject00.r",
        ])
        .passes()
        .unwrap();
        assert_eq!(
            p.iter().map(|p| p.socket_type).collect::<Vec<_>>(),
            [
                "NodeSocketColor",
                "NodeSocketVector",
                "NodeSocketVector4D",
                "NodeSocketVector",
                "NodeSocketColor",
            ]
        );
        assert_eq!(p[0].components, ["B", "R", "G"]);
        assert_eq!(p[1].components, ["Z", "X", "Y"]);
        assert_eq!(p[4].components, ["a", "b", "g", "r"]);
        assert_eq!(
            header(&[
                "L.Unknown.C1",
                "L.Unknown.C2",
                "L.Unknown.C3",
                "L.Unknown.C4"
            ])
            .passes()
            .unwrap_err()
            .kind,
            "exr_unsupported"
        );
    }
    #[test]
    fn unlayered_file_output_groups_have_a_qualified_generic_interpretation() {
        let p = header(&["Image.R", "Image.G", "Image.B", "Image.A", "Custom.Z"])
            .passes()
            .unwrap();
        assert_eq!(names(&p), ["Image", "Custom"]);
        assert_eq!(p[0].source_pass, "Image");
    }
    #[test]
    fn ambiguous_or_unqualified_layouts_refuse() {
        for input in [
            vec!["R", "G", "B"],
            vec!["A.Depth.Z", "B.Depth.Z"],
            vec!["Depth.Z", "L.Normal.X"],
            vec!["L..Z"],
            vec!["L.Combined.Z"],
            vec!["L.Combined.R", "L.Combined.R"],
        ] {
            assert!(header(&input).passes().is_err(), "{input:?}");
        }
        for flag in [0x800, 0x1000] {
            let mut h = header(&["L.Depth.Z"]);
            h.version |= flag;
            assert_eq!(h.passes().unwrap_err().kind, "exr_unsupported");
        }
        for key in [b"multiView".as_slice(), b"view"] {
            let mut h = header(&["L.Depth.Z"]);
            h.parts[0].values.insert(key.to_vec(), b"left".to_vec());
            assert_eq!(h.passes().unwrap_err().kind, "exr_unsupported");
        }
        let mut h = header(&["L.Depth.Z"]);
        h.parts[0].channels[0].sampling = [2, 1];
        assert_eq!(h.passes().unwrap_err().kind, "exr_unsupported");
        let h = ExrHeader {
            version: 2,
            parts: Vec::new(),
            extent: 0,
        };
        assert_eq!(h.passes().unwrap_err().kind, "exr_unsupported");
    }
}
