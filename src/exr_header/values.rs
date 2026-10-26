//! Attribute payloads only; offsets and declared lengths belong to the caller.
//! Preview decoding needs only its eight-byte dimensions prefix, never pixels.
//! Layout reference: https://openexr.com/en/latest/OpenEXRFileLayout.html
//! Independently implemented from its Predefined Attribute Types table.
use serde_json::{Value, json};
#[derive(Clone, Copy)]
enum Refusal {
    UnknownType,
    Length,
    Value,
    Nonfinite,
}
impl Refusal {
    fn kind(self) -> &'static str {
        match self {
            Self::UnknownType => "unknown_attribute_type",
            Self::Length => "attribute_length",
            Self::Value => "attribute_value",
            Self::Nonfinite => "nonfinite_attribute_float",
        }
    }
}
type Decoded = std::result::Result<Value, Refusal>;
fn numbers(bytes: &[u8], width: usize, count: usize, float: bool) -> Decoded {
    if bytes.len() != width * count {
        return Err(Refusal::Length);
    }
    bytes
        .chunks_exact(width)
        .map(|b| {
            if float {
                let n = if width == 4 {
                    f32::from_le_bytes(b.try_into().unwrap()) as f64
                } else {
                    f64::from_le_bytes(b.try_into().unwrap())
                };
                serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .ok_or(Refusal::Nonfinite)
            } else {
                Ok(json!(i32::from_le_bytes(b.try_into().unwrap())))
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map(Value::Array)
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn string_value(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes);
    json!({"text":text.ok(),"utf8_valid":text.is_ok(),"hex":crate::value::hex(bytes)})
}
fn typed(kind: &[u8], bytes: &[u8], name_max: usize) -> Decoded {
    match kind {
        b"string" => Ok(string_value(bytes)),
        b"stringvector" => {
            let mut rest = bytes;
            let mut strings = Vec::new();
            while !rest.is_empty() {
                if rest.len() < 4 {
                    return Err(Refusal::Length);
                }
                let length = i32::from_le_bytes(rest[..4].try_into().unwrap());
                if length < 0 {
                    return Err(Refusal::Value);
                }
                rest = &rest[4..];
                let text = rest.get(..length as usize).ok_or(Refusal::Length)?;
                strings.push(string_value(text));
                rest = &rest[text.len()..];
            }
            Ok(json!(strings))
        }
        b"bytes" => {
            if bytes.len() < 4 {
                return Err(Refusal::Length);
            }
            let hint_length = u32_at(bytes, 0) as usize;
            let rest = &bytes[4..];
            let hint = rest.get(..hint_length).ok_or(Refusal::Length)?;
            Ok(json!({"typeHint":string_value(hint),"hex":crate::value::hex(&rest[hint.len()..])}))
        }
        b"timecode" => {
            if bytes.len() != 8 {
                return Err(Refusal::Length);
            }
            // File Layout defines packed words, not a frame-rate interpretation.
            Ok(json!({"timeAndFlags":u32_at(bytes,0),"userData":u32_at(bytes,4)}))
        }
        b"keycode" => {
            let ns = numbers(bytes, 4, 7, false)?;
            let ns = ns.as_array().unwrap();
            Ok(
                json!({"filmMfcCode":ns[0],"filmType":ns[1],"prefix":ns[2],"count":ns[3],"perfOffset":ns[4],"perfsPerFrame":ns[5],"perfsPerCount":ns[6]}),
            )
        }
        b"rational" => {
            if bytes.len() != 8 {
                return Err(Refusal::Length);
            }
            // Preserve the serialized fraction, including a zero denominator.
            Ok(
                json!({"numerator":i32::from_le_bytes(bytes[..4].try_into().unwrap()),"denominator":u32_at(bytes,4)}),
            )
        }
        b"tiledesc" => {
            if bytes.len() != 9 {
                return Err(Refusal::Length);
            }
            let mode = bytes[8];
            let level = mode & 15;
            let rounding = mode >> 4;
            let level_name = ["one_level", "mipmap_levels", "ripmap_levels"]
                .get(level as usize)
                .ok_or(Refusal::Value)?;
            let rounding_name = ["round_down", "round_up"]
                .get(rounding as usize)
                .ok_or(Refusal::Value)?;
            Ok(
                json!({"xSize":u32_at(bytes,0),"ySize":u32_at(bytes,4),"mode":mode,"levelMode":{"code":level,"name":level_name},"roundingMode":{"code":rounding,"name":rounding_name}}),
            )
        }
        b"preview" => {
            if bytes.len() < 8 {
                return Err(Refusal::Length);
            }
            // Parent validates declared extent and can supply just this prefix.
            Ok(json!({"width":u32_at(bytes,0),"height":u32_at(bytes,4)}))
        }
        b"chromaticities" => {
            let ns = numbers(bytes, 4, 8, true)?;
            let ns = ns.as_array().unwrap();
            Ok(
                json!({"redX":ns[0],"redY":ns[1],"greenX":ns[2],"greenY":ns[3],"blueX":ns[4],"blueY":ns[5],"whiteX":ns[6],"whiteY":ns[7]}),
            )
        }
        b"int" | b"float" | b"double" => {
            let mut ns = numbers(
                bytes,
                if kind == b"double" { 8 } else { 4 },
                1,
                kind != b"int",
            )?;
            Ok(ns.as_array_mut().unwrap().remove(0))
        }
        b"v2i" => numbers(bytes, 4, 2, false),
        b"v2f" => numbers(bytes, 4, 2, true),
        b"v3i" => numbers(bytes, 4, 3, false),
        b"v3f" => numbers(bytes, 4, 3, true),
        b"box2i" | b"box2f" => {
            let ns = numbers(bytes, 4, 4, kind == b"box2f")?;
            let ns = ns.as_array().unwrap();
            Ok(json!({"min":&ns[..2],"max":&ns[2..]}))
        }
        b"m33f" | b"m44f" => {
            let side = if kind == b"m33f" { 3 } else { 4 };
            let ns = numbers(bytes, 4, side * side, true)?;
            Ok(Value::Array(
                ns.as_array()
                    .unwrap()
                    .chunks_exact(side)
                    .map(|r| json!(r))
                    .collect(),
            ))
        }
        b"compression" | b"lineOrder" | b"envmap" => {
            if bytes.len() != 1 {
                return Err(Refusal::Length);
            }
            // Enum codes: OpenEXR File Layout; LJ2K/ZSTD are also confirmed by
            // official OpenEXR 3.5.2 Python enum values and Technical Introduction.
            let names: &[&str] = if kind == b"compression" {
                &[
                    "none", "rle", "zips", "zip", "piz", "pxr24", "b44", "b44a", "dwaa", "dwab",
                    "htj2k256", "htj2k32", "lj2k", "zstd",
                ]
            } else if kind == b"lineOrder" {
                &["increasing_y", "decreasing_y", "random_y"]
            } else {
                &["latlong", "cube"]
            };
            let name = names.get(bytes[0] as usize).ok_or(Refusal::Value)?;
            Ok(json!({"code":bytes[0],"name":name}))
        }
        b"chlist" => {
            let channels = super::read_channels(bytes, name_max).map_err(|_| Refusal::Value)?;
            Ok(Value::Array(channels.iter().map(|c| {
                let pixel_type_name = ["uint","half","float"][c.pixel_type as usize];
                json!({"name_hex":crate::value::hex(&c.name),"name":std::str::from_utf8(&c.name).ok(),"pixel_type":c.pixel_type,"pixel_type_name":pixel_type_name,"linear":c.linear,"pLinear":c.linear!=0,"reserved":c.reserved,"sampling":c.sampling,"xSampling":c.sampling[0],"ySampling":c.sampling[1]})
            }).collect()))
        }
        _ => Err(Refusal::UnknownType),
    }
}
pub(super) fn decode(kind: &[u8], bytes: &[u8], name_max: usize) -> Value {
    match typed(kind, bytes, name_max) {
        Ok(value) => json!({"status":"decoded","decoded":true,"value":value}),
        Err(reason) => {
            json!({"status":"refused","decoded":false,"kind":reason.kind(),"raw_hex":crate::value::hex(bytes)})
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn preview_accepts_only_the_dimensions_prefix_without_reading_pixels() {
        let prefix = [2, 0, 0, 0, 1, 0, 0, 0];
        assert_eq!(
            super::decode(b"preview", &prefix, 31),
            serde_json::json!({"status":"decoded","decoded":true,"value":{"width":2,"height":1}})
        );
        let short = super::decode(b"preview", &prefix[..7], 31);
        assert_eq!(short["decoded"], false);
        assert_eq!(short["kind"], "attribute_length");
    }
}
