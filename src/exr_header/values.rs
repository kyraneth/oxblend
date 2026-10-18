//! Attribute payloads only: callers supply the exact declared-size byte slice.
//! OpenEXR File Layout and v3.4.5 enum headers define widths/codes below.
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
fn typed(kind: &[u8], bytes: &[u8], name_max: usize) -> Decoded {
    match kind {
        b"string" => Ok(json!({"text":std::str::from_utf8(bytes).ok(),"utf8_valid":std::str::from_utf8(bytes).is_ok(),"hex":crate::value::hex(bytes)})),
        b"int" | b"float" | b"double" => {
            let mut ns=numbers(bytes,if kind==b"double" {8}else{4},1,kind!=b"int")?;
            Ok(ns.as_array_mut().unwrap().remove(0))
        }
        b"v2i" => numbers(bytes,4,2,false),
        b"v2f" => numbers(bytes,4,2,true),
        b"v3i" => numbers(bytes,4,3,false),
        b"v3f" => numbers(bytes,4,3,true),
        b"box2i" | b"box2f" => {
            let ns=numbers(bytes,4,4,kind==b"box2f")?;let ns=ns.as_array().unwrap();
            Ok(json!({"min":&ns[..2],"max":&ns[2..]}))
        }
        b"m33f" | b"m44f" => {
            let side=if kind==b"m33f" {3}else{4};let ns=numbers(bytes,4,side*side,true)?;
            Ok(Value::Array(ns.as_array().unwrap().chunks_exact(side).map(|r|json!(r)).collect()))
        }
        b"compression" | b"lineOrder" => {
            if bytes.len()!=1 { return Err(Refusal::Length); }
            // OpenEXR v3.4.5 ImfCompression.h and ImfLineOrder.h, not guessed ranges.
            let names:&[&str]=if kind==b"compression" {
                &["none","rle","zips","zip","piz","pxr24","b44","b44a","dwaa","dwab","htj2k256","htj2k32"]
            } else { &["increasing_y","decreasing_y","random_y"] };
            let name=names.get(bytes[0] as usize).ok_or(Refusal::Value)?;
            Ok(json!({"code":bytes[0],"name":name}))
        }
        b"chlist" => super::read_channels(bytes,name_max).map(|channels|json!(channels.iter().map(|c|json!({"name_hex":crate::value::hex(&c.name),"name":std::str::from_utf8(&c.name).ok(),"pixel_type":c.pixel_type,"linear":c.linear,"reserved":c.reserved,"sampling":c.sampling})).collect::<Vec<_>>())).map_err(|_|Refusal::Value),
        _ => Err(Refusal::UnknownType),
    }
}
pub(super) fn decode(kind: &[u8], bytes: &[u8], name_max: usize) -> Value {
    match typed(kind, bytes, name_max) {
        Ok(value) => json!({"status":"decoded","value":value}),
        Err(reason) => {
            json!({"status":"refused","kind":reason.kind(),"raw_hex":crate::value::hex(bytes)})
        }
    }
}
