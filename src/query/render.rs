//! Saved Scene render controls, not runtime defaults or evaluated output.
//! Authored field/bit facts: Blender v5.2.1 DNA_scene_types.h and DNA_color_types.h.
//! Native 5.2.1 toggles qualify shading shadow/cavity and Eevee raytracing bits.
//! Minimal enum mappings are enabled only for qualified saved values; unknown
//! representations remain unavailable instead of substituting defaults.

use crate::query::access::{Record, float, hex_float, int, text};
use crate::{BlendFile, Error, Result};
use serde_json::{Map, Value as Json, json};

const CYCLES_KEYS: &[&str] = &[
    "samples",
    "preview_samples",
    "use_adaptive_sampling",
    "adaptive_threshold",
    "adaptive_min_samples",
    "use_denoising",
    "denoiser",
    "device",
    "seed",
    "use_animated_seed",
    "max_bounces",
    "diffuse_bounces",
    "glossy_bounces",
    "transmission_bounces",
    "volume_bounces",
    "transparent_max_bounces",
    "sample_clamp_direct",
    "sample_clamp_indirect",
];

fn gate_52(file: &BlendFile) -> Result<()> {
    if file.header().version != 502 {
        return Err(Error::new(
            "unverified_semantics",
            format!(
                "only 5.2 saved representation qualified (header {})",
                file.header().version
            ),
        ));
    }
    let sub = crate::query::access::records(file, "FileGlobal")?
        .first()
        .map(|r| int(file, r, "subversion"))
        .transpose()?
        .unwrap_or(-1);
    if sub != 44 {
        if sub == 45 {
            return Err(Error::new(
                "unverified_semantics",
                "forward-file subversion 45 is not proof 44 understands all of 45",
            ));
        }
        return Err(Error::new(
            "unverified_semantics",
            format!("old-version gate red (subversion {sub}); 5.2.44 golden first"),
        ));
    }
    Ok(())
}

fn media_type(v: i64) -> Result<&'static str> {
    match v {
        0 => Ok("IMAGE"),
        1 => Ok("MULTI_LAYER_IMAGE"),
        2 => Ok("VIDEO"),
        _ => Err(Error::new(
            "unverified_semantics",
            format!("media_type {v} unqualified"),
        )),
    }
}

fn file_format(v: i64) -> Result<&'static str> {
    // Observed 5.2.1 golden values only (17/23/28); others stay unqualified.
    match v {
        17 => Ok("PNG"),
        23 => Ok("OPEN_EXR"),
        // Qualified by saved FFMPEG controls in the native-reopened fixtures.
        24 => Ok("FFMPEG"),
        28 => Ok("OPEN_EXR_MULTILAYER"),
        _ => Err(Error::new(
            "unverified_semantics",
            format!("imtype {v} unqualified (golden covers PNG/OPEN_EXR/MULTILAYER)"),
        )),
    }
}

fn color_mode(v: i64) -> Result<&'static str> {
    match v {
        24 => Ok("RGB"),
        32 => Ok("RGBA"),
        _ => Err(Error::new(
            "unverified_semantics",
            format!("planes {v} unqualified (golden covers RGBA only)"),
        )),
    }
}

fn color_depth(v: i64) -> Result<&'static str> {
    match v {
        2 => Ok("8"),
        16 => Ok("16"),
        64 => Ok("32"),
        _ => Err(Error::new(
            "unverified_semantics",
            format!("depth {v} unqualified (golden covers 8/16/32)"),
        )),
    }
}

fn exr_codec(v: i64) -> Result<&'static str> {
    match v {
        0 => Ok("NONE"),
        2 => Ok("ZIP"),
        _ => Err(Error::new(
            "unverified_semantics",
            format!("exr_codec {v} unqualified (golden covers NONE/ZIP)"),
        )),
    }
}

fn color_management(v: i64) -> Result<&'static str> {
    match v {
        0 => Ok("FOLLOW_SCENE"),
        _ => Err(Error::new(
            "unverified_semantics",
            format!("color_management {v} unqualified"),
        )),
    }
}

pub(super) fn image_format(file: &BlendFile, base: &Record, prefix: &str) -> Result<Json> {
    let at = |f: &str| {
        if prefix.is_empty() {
            f.to_owned()
        } else {
            format!("{prefix}.{f}")
        }
    };
    let mt = int(file, base, &at("media_type"))?;
    let it = int(file, base, &at("imtype"))?;
    let dp = int(file, base, &at("depth"))?;
    let pl = int(file, base, &at("planes"))?;
    let qu = int(file, base, &at("quality"))?;
    let co = int(file, base, &at("compress"))?;
    let ex = int(file, base, &at("exr_codec"))?;
    let cm = int(file, base, &at("color_management"))?;
    Ok(json!({
        "media_type": media_type(mt)?,
        "file_format": file_format(it)?,
        "color_mode": color_mode(pl)?,
        "color_depth": color_depth(dp)?,
        "exr_codec": exr_codec(ex)?,
        "compression": co,
        "quality": qu,
        "color_management": color_management(cm)?,
    }))
}

fn cycles_explicit(file: &BlendFile, record: &Record) -> Result<Map<String, Json>> {
    let props = crate::query::props::decode_owner(
        file,
        record,
        "id.properties",
        Some("id.system_properties"),
    )?;
    let sys = props
        .get("system_entries")
        .and_then(|v| v.as_object())
        .ok_or_else(|| Error::new("type", "system_entries"))?;
    let user = props
        .get("entries")
        .and_then(|v| v.as_object())
        .ok_or_else(|| Error::new("type", "entries"))?;
    // System precedence; malformed system never falls back to user.
    if let Some(cycles) = sys.get("cycles") {
        let entries = cycles
            .get("entries")
            .and_then(|v| v.as_object())
            .ok_or_else(|| Error::new("type", "system cycles group malformed; no user fallback"))?;
        // `cycles` must be a group; a scalar here is malformed, not a fallback.
        if cycles.get("kind") != Some(&json!("group")) {
            return Err(Error::new(
                "type",
                "system cycles is not a group; no user fallback",
            ));
        }
        let mut out = Map::new();
        for k in CYCLES_KEYS {
            if let Some(v) = entries.get(*k) {
                out.insert((*k).to_owned(), v.clone());
            }
        }
        return Ok(out);
    }
    if let Some(cycles) = user.get("cycles") {
        if cycles.get("kind") != Some(&json!("group")) {
            return Err(Error::new("type", "user cycles is not a group"));
        }
        let entries = cycles
            .get("entries")
            .and_then(|v| v.as_object())
            .ok_or_else(|| Error::new("type", "user cycles group"))?;
        let mut out = Map::new();
        for k in CYCLES_KEYS {
            if let Some(v) = entries.get(*k) {
                out.insert((*k).to_owned(), v.clone());
            }
        }
        return Ok(out);
    }
    Ok(Map::new())
}

/// Whole `render` column for one Scene row. No defaults; any unqualified
/// saved value refuses the column (parent reports unavailable with the exact
/// site), never a guessed zero/false/empty.
pub fn project(file: &BlendFile, record: &Record) -> Result<Json> {
    gate_52(file)?;
    let resolution = json!({
        "resolution_x": int(file, record, "r.xsch")?,
        "resolution_y": int(file, record, "r.ysch")?,
        "resolution_percentage": int(file, record, "r.size")?,
        "pixel_aspect_x": float(file, record, "r.xasp")?,
        "pixel_aspect_y": float(file, record, "r.yasp")?,
    });
    let frame_range = json!({
        "frame_start": int(file, record, "r.sfra")?,
        "frame_end": int(file, record, "r.efra")?,
        "frame_step": int(file, record, "r.frame_step")?,
    });
    let numerator = int(file, record, "r.frs_sec")?;
    let base = float(file, record, "r.frs_sec_base")?;
    let base_hex = hex_float(base);
    let effective_hex = if base == 0.0 || !base.is_finite() {
        Json::Null
    } else {
        Json::String(hex_float(numerator as f64 / base))
    };
    let fps = json!({"numerator":numerator,"base_hex":base_hex,"effective_hex":effective_hex});
    let output = {
        let filepath = text(file, record, "r.pic")?;
        let format = image_format(file, record, "r.im_format")?;
        let scemode = int(file, record, "r.scemode")?;
        let mode = int(file, record, "r.mode")?;
        // R_EXTENSION=1<<4, R_NO_OVERWRITE=1<<22, R_TOUCH=1<<23.
        let use_file_extension = (scemode & (1 << 4)) != 0;
        let use_overwrite = (mode & (1 << 22)) == 0;
        let use_placeholder = (mode & (1 << 23)) != 0;
        json!({"filepath":filepath,"format":format,
            "use_file_extension":use_file_extension,
            "use_overwrite":use_overwrite,
            "use_placeholder":use_placeholder})
    };
    let engine = text(file, record, "r.engine")?;
    let common = {
        let alphamode = int(file, record, "r.alphamode")?;
        let film_transparent = match alphamode {
            0 => false, // R_ADDSKY
            1 => true,  // R_ALPHAPREMUL
            _ => {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("alphamode {alphamode} unqualified"),
                ));
            }
        };
        let scemode = int(file, record, "r.scemode")?;
        let mode = int(file, record, "r.mode")?;
        // R_DOCOMP=1<<6, R_DOSEQ=1<<0, R_BORDER=1<<9, R_CROP=1<<11,
        // R_MBLUR=1<<14.
        let use_compositing = (scemode & (1 << 6)) != 0;
        let use_sequencer = (scemode & (1 << 0)) != 0;
        let use_border = (mode & (1 << 9)) != 0;
        let use_crop_to_border = (mode & (1 << 11)) != 0;
        let border_min_x = float(file, record, "r.border.xmin")?;
        let border_max_x = float(file, record, "r.border.xmax")?;
        let border_min_y = float(file, record, "r.border.ymin")?;
        let border_max_y = float(file, record, "r.border.ymax")?;
        let use_motion_blur = (mode & (1 << 14)) != 0;
        let motion_blur_shutter = float(file, record, "r.blurfac")?;
        let motion_blur_position = match int(file, record, "r.motion_blur_position")? {
            0 => "CENTER", // SCE_MB_CENTER
            1 => "START",
            2 => "END",
            v => {
                return Err(Error::new(
                    "unverified_semantics",
                    format!("motion_blur_position {v} unqualified"),
                ));
            }
        };
        json!({"film_transparent":film_transparent,
            "use_compositing":use_compositing,"use_sequencer":use_sequencer,
            "use_border":use_border,"use_crop_to_border":use_crop_to_border,
            "border_min_x":border_min_x,"border_min_y":border_min_y,
            "border_max_x":border_max_x,"border_max_y":border_max_y,
            "use_motion_blur":use_motion_blur,
            "motion_blur_shutter":motion_blur_shutter,
            "motion_blur_position":motion_blur_position})
    };
    let engine_settings = {
        let cycles = cycles_explicit(file, record)?;
        // Eevee 5.2.1 RNA: taa samples + raytracing only (use_gtao/gtao_distance
        // absent via hasattr; deprecated DNA ignored to match oracle).
        let eevee = {
            let mut m = Map::new();
            m.insert(
                "taa_render_samples".to_owned(),
                json!(int(file, record, "eevee.taa_render_samples")?),
            );
            m.insert(
                "taa_samples".to_owned(),
                json!(int(file, record, "eevee.taa_samples")?),
            );
            // Bit14 observed via native toggle (base 0x11110862 vs ray-true
            // 0x11114862); other flag bits stay uninterpreted.
            let flag = int(file, record, "eevee.flag")?;
            m.insert("use_raytracing".to_owned(), json!((flag & (1 << 14)) != 0));
            Json::Object(m)
        };
        // Workbench 5.2.1 RNA: light/color_type/shadows/cavity only
        // (show_outline absent via hasattr, omitted). Light 1=STUDIO,
        // color_type 0=MATERIAL observed; others unqualified.
        let workbench = {
            let light = match int(file, record, "display.shading.light")? {
                1 => "STUDIO",
                v => {
                    return Err(Error::new(
                        "unverified_semantics",
                        format!("shading light {v} unqualified"),
                    ));
                }
            };
            let color_type = match int(file, record, "display.shading.color_type")? {
                0 => "MATERIAL",
                v => {
                    return Err(Error::new(
                        "unverified_semantics",
                        format!("shading color_type {v} unqualified"),
                    ));
                }
            };
            // Bits observed via native toggle: bit2 shadows (0x10->0x14),
            // bit5 cavity (0x10->0x30).
            let flag = int(file, record, "display.shading.flag")?;
            json!({"light":light,"color_type":color_type,
                "show_shadows":(flag & (1 << 2)) != 0,
                "show_cavity":(flag & (1 << 5)) != 0})
        };
        json!({"cycles_explicit":cycles,"eevee":eevee,"workbench":workbench})
    };
    let color_management = {
        let flag = int(file, record, "view_settings.flag")?;
        // COLORMANAGE_VIEW_USE_CURVES=1<<0, USE_WHITE_BALANCE=1<<2.
        let view = json!({
            "view_transform": text(file, record, "view_settings.view_transform")?,
            "look": text(file, record, "view_settings.look")?,
            "exposure": float(file, record, "view_settings.exposure")?,
            "gamma": float(file, record, "view_settings.gamma")?,
            "use_curve_mapping": (flag & (1 << 0)) != 0,
            "use_white_balance": (flag & (1 << 2)) != 0,
            "white_balance_temperature": float(file, record, "view_settings.temperature")?,
            "white_balance_tint": float(file, record, "view_settings.tint")?,
        });
        let display =
            json!({"display_device": text(file, record, "display_settings.display_device")?});
        let sequencer = json!({"name": text(file, record, "sequencer_colorspace_settings.name")?});
        json!({"view":view,"display":display,"sequencer":sequencer})
    };
    Ok(
        json!({"resolution":resolution,"frame_range":frame_range,"fps":fps,
        "output":output,"engine":engine,"common":common,
        "engine_settings":engine_settings,"color_management":color_management}),
    )
}
