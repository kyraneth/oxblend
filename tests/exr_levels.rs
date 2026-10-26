//! Offset counts/coordinates across compression block heights and tile levels.
use serde_json::Value;
use std::process::Command;
#[path = "support/scratch.rs"]
mod scratch;
fn attr(b: &mut Vec<u8>, n: &str, k: &str, v: &[u8]) {
    b.extend(n.bytes());
    b.push(0);
    b.extend(k.bytes());
    b.push(0);
    b.extend((v.len() as i32).to_le_bytes());
    b.extend(v);
}
fn prefix(comp: u8, width: i32, height: i32, tile: Option<u8>) -> Vec<u8> {
    let mut b = 20000630u32.to_le_bytes().to_vec();
    b.extend((2u32 | if tile.is_some() { 0x200 } else { 0 }).to_le_bytes());
    let mut c = b"R\0".to_vec();
    c.extend(1i32.to_le_bytes());
    c.extend([0; 4]);
    c.extend(1i32.to_le_bytes());
    c.extend(1i32.to_le_bytes());
    c.push(0);
    attr(&mut b, "channels", "chlist", &c);
    attr(&mut b, "compression", "compression", &[comp]);
    for n in ["dataWindow", "displayWindow"] {
        attr(
            &mut b,
            n,
            "box2i",
            &[-3i32, -2, width - 4, height - 3]
                .into_iter()
                .flat_map(i32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    attr(&mut b, "lineOrder", "lineOrder", &[1]);
    attr(&mut b, "pixelAspectRatio", "float", &1f32.to_le_bytes());
    attr(&mut b, "screenWindowWidth", "float", &1f32.to_le_bytes());
    attr(&mut b, "screenWindowCenter", "v2f", &[0; 8]);
    if let Some(mode) = tile {
        let mut t = 2u32.to_le_bytes().repeat(2);
        t.push(mode);
        attr(&mut b, "tiles", "tiledesc", &t);
    }
    b.push(0);
    b
}
fn finish(mut b: Vec<u8>, chunks: &[Vec<i32>]) -> Vec<u8> {
    let mut offset = b.len() as u64 + chunks.len() as u64 * 8;
    for c in chunks {
        b.extend(offset.to_le_bytes());
        offset += c.len() as u64 * 4 + 5;
    }
    for c in chunks {
        for v in c {
            b.extend(v.to_le_bytes());
        }
        b.extend(1i32.to_le_bytes());
        b.push(0);
    }
    b
}
#[test]
fn block_heights_levels_rounding_and_bad_offsets() {
    let dir = scratch::scratch_dir(&std::env::temp_dir());
    // 257 lines crosses every format-defined block height including DWAB/HTJ2K256.
    for (comp, lines) in [1i32, 1, 1, 16, 32, 16, 32, 32, 32, 256, 256, 32, 256, 1]
        .iter()
        .enumerate()
    {
        let chunks = (0..257i32.div_euclid(*lines) + i32::from(257 % lines != 0))
            .map(|i| vec![-2 + i * lines])
            .collect::<Vec<_>>();
        let b = finish(prefix(comp as u8, 1, 257, None), &chunks);
        std::fs::write(dir.join(format!("scan-{comp}.exr")), b).unwrap();
    }
    for up in [false, true] {
        for mode in 0..3 {
            // Independent, explicit 5x3 dimension sequences exercise non-power-of-two rounding.
            let xs = if up {
                vec![5i32, 3, 2, 1]
            } else {
                vec![5i32, 2, 1]
            };
            let ys = if up { vec![3i32, 2, 1] } else { vec![3i32, 1] };
            let levels = match mode {
                0 => vec![(0, 0)],
                1 => (0..xs.len().max(ys.len())).map(|i| (i, i)).collect(),
                _ => (0..ys.len())
                    .flat_map(|y| (0..xs.len()).map(move |x| (x, y)))
                    .collect(),
            };
            let mut chunks = Vec::new();
            for (x, y) in levels {
                for ty in 0..(ys.get(y).copied().unwrap_or(1) + 1) / 2 {
                    for tx in 0..(xs.get(x).copied().unwrap_or(1) + 1) / 2 {
                        chunks.push(vec![tx, ty, x as i32, y as i32]);
                    }
                }
            }
            let b = finish(
                prefix(3, 5, 3, Some(mode + if up { 16 } else { 0 })),
                &chunks,
            );
            std::fs::write(dir.join(format!("tile-{up}-{mode}.exr")), &b).unwrap();
            let mut bad = b.clone();
            let end = prefix(3, 5, 3, Some(mode + if up { 16 } else { 0 })).len();
            // Duplicate offsets must not pass even when both point at readable headers.
            let offset = bad[end..end + 8].to_vec();
            bad[end + 8..end + 16].copy_from_slice(&offset);
            std::fs::write(dir.join(format!("bad-{up}-{mode}.exr")), bad).unwrap();
        }
    }
    // Format-derived hostile geometry: an incomplete table for i32::MAX
    // scanlines. A sparse file must not be materialized as guessed offsets.
    let sparse_header = prefix(0, 1, i32::MAX, None);
    let sparse = dir.join("incomplete-huge-table.exr");
    std::fs::write(&sparse, &sparse_header).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&sparse)
        .unwrap()
        .set_len(sparse_header.len() as u64 + i32::MAX as u64 * 8 - 1)
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_oxblend"))
        .arg("exr-scan")
        .arg(&dir)
        .args(["--jobs", "1"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    for r in rows.iter().filter(|r| r["record_type"] == "file") {
        if r["relative_path"] == "incomplete-huge-table.exr" {
            assert_eq!(r["status"], "header_only_no_chunks");
            assert_eq!(r["chunks_present"], 0);
            continue;
        }
        let bad = r["relative_path"].as_str().unwrap().starts_with("bad-");
        assert_eq!(
            r["status"],
            if bad { "chunks_truncated" } else { "ok" },
            "{r}"
        );
        assert_eq!(r["placeholder_1x1"], false);
        if !bad {
            assert_eq!(r["chunks_declared"], r["chunks_present"], "{r}");
            assert_eq!(r["bytes_expected"], r["file_size"], "{r}");
        }
    }
    assert_eq!(rows.last().unwrap()["counts_by_status"]["ok"], 20);
    assert_eq!(
        rows.last().unwrap()["counts_by_status"]["chunks_truncated"],
        6
    );
    std::fs::remove_dir_all(dir).unwrap();
}
