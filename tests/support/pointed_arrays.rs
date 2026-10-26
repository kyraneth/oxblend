//! Deterministic pointed-array wire fixture shared by reader and malformed tests.
//! This proves checked serialized reads, not native Blender open validity.
fn write_int(out: &mut [u8], offset: usize, width: usize, value: u64, little: bool) {
    for i in 0..width {
        let byte = (value >> (8 * i)) as u8;
        let at = if little {
            offset + i
        } else {
            offset + width - 1 - i
        };
        out[at] = byte;
    }
}

fn push_u16(out: &mut Vec<u8>, value: u16, little: bool) {
    let start = out.len();
    out.resize(start + 2, 0);
    write_int(out, start, 2, u64::from(value), little);
}
fn push_u32(out: &mut Vec<u8>, value: u32, little: bool) {
    let start = out.len();
    out.resize(start + 4, 0);
    write_int(out, start, 4, u64::from(value), little);
}
fn align(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// SDNA for the schema above. `Widget` is 9*ps + 4 bytes; `Part` embeds the
/// 8-byte `ID` by value so entries resolve through the ID namespace exactly
/// like saved `Mesh.mat -> Material` tables.
fn dna_payload(little: bool, ps: usize, callable: bool) -> Vec<u8> {
    let widget_len = 9 * ps + 4;
    let names: [&[u8]; 13] = [
        if callable {
            &b"(*label)()"[..]
        } else {
            &b"*label"[..]
        },
        &b"totcol"[..],
        if callable {
            &b"(**parts)()"[..]
        } else {
            &b"**parts"[..]
        },
        &b"*raw"[..],
        &b"*single"[..],
        &b"*numbers"[..],
        &b"*arr[2]"[..],
        &b"**blob"[..],
        &b"*ibytes"[..],
        &b"id"[..],
        &b"x"[..],
        &b"a"[..],
        &b"b"[..],
    ];
    let types: [(&[u8], usize); 8] = [
        (b"char", 1),
        (b"int", 4),
        (b"uchar", 1),
        (b"Part", 12),
        (b"Widget", widget_len),
        (b"ID", 8),
        (b"Other", 4),
        (b"int8_t", 1),
    ];
    let mut out = b"SDNA".to_vec();
    out.extend_from_slice(b"NAME");
    push_u32(&mut out, names.len() as u32, little);
    for name in names {
        out.extend_from_slice(name);
        out.push(0);
    }
    align(&mut out);
    out.extend_from_slice(b"TYPE");
    push_u32(&mut out, types.len() as u32, little);
    for (ty, _) in types {
        out.extend_from_slice(ty);
        out.push(0);
    }
    align(&mut out);
    out.extend_from_slice(b"TLEN");
    for (_, len) in types {
        push_u16(&mut out, len as u16, little);
    }
    align(&mut out);
    out.extend_from_slice(b"STRC");
    // STRC order fixes struct indices: Widget=0, Part=1, ID=2, Other=3.
    push_u32(&mut out, 4, little);
    let structure = |out: &mut Vec<u8>, ty: u16, fields: &[(u16, u16)]| {
        push_u16(out, ty, little);
        push_u16(out, fields.len() as u16, little);
        for (ft, fname) in fields {
            push_u16(out, *ft, little);
            push_u16(out, *fname, little);
        }
    };
    structure(
        &mut out,
        4,
        &[
            (0, 0),
            (1, 1),
            (3, 2),
            (2, 3),
            (3, 4),
            (1, 5),
            (0, 6),
            (0, 7),
            (7, 8),
        ],
    );
    structure(&mut out, 3, &[(5, 9), (1, 10)]);
    structure(&mut out, 5, &[(1, 11), (1, 12)]);
    structure(&mut out, 6, &[(1, 10)]);
    out
}

#[derive(Clone, Default)]
struct W {
    label: u64,
    totcol: u32,
    parts: u64,
    raw: u64,
    single: u64,
    numbers: u64,
    arr: [u64; 2],
    blob: u64,
    ibytes: u64,
}

fn widget_record(little: bool, ps: usize, w: &W) -> Vec<u8> {
    let mut r = vec![0u8; 9 * ps + 4];
    write_int(&mut r, 0, ps, w.label, little);
    write_int(&mut r, ps, 4, u64::from(w.totcol), little);
    write_int(&mut r, ps + 4, ps, w.parts, little);
    write_int(&mut r, 2 * ps + 4, ps, w.raw, little);
    write_int(&mut r, 3 * ps + 4, ps, w.single, little);
    write_int(&mut r, 4 * ps + 4, ps, w.numbers, little);
    write_int(&mut r, 5 * ps + 4, ps, w.arr[0], little);
    write_int(&mut r, 6 * ps + 4, ps, w.arr[1], little);
    write_int(&mut r, 7 * ps + 4, ps, w.blob, little);
    write_int(&mut r, 8 * ps + 4, ps, w.ibytes, little);
    r
}

fn table(little: bool, ps: usize, entries: &[u64]) -> Vec<u8> {
    let mut t = vec![0u8; entries.len() * ps];
    for (i, entry) in entries.iter().enumerate() {
        write_int(&mut t, i * ps, ps, *entry, little);
    }
    t
}

struct Synth {
    little: bool,
    ps: usize,
    out: Vec<u8>,
}

impl Synth {
    fn new(little: bool, ps: usize) -> Self {
        let mut out = b"BLENDER".to_vec();
        out.push(if ps == 4 { b'_' } else { b'-' });
        out.push(if little { b'v' } else { b'V' });
        out.extend_from_slice(b"405");
        Self { little, ps, out }
    }
    fn put(&mut self, width: usize, value: u64) {
        let start = self.out.len();
        self.out.resize(start + width, 0);
        write_int(&mut self.out, start, width, value, self.little);
    }
    fn head(&mut self, code: &[u8; 4], address: u64, sdna: usize, count: usize, payload: &[u8]) {
        self.out.extend_from_slice(code);
        self.put(4, payload.len() as u64);
        self.put(self.ps, address);
        self.put(4, sdna as u64);
        self.put(4, count as u64);
        self.out.extend_from_slice(payload);
    }
    fn widget(&mut self, address: u64, w: &W) {
        let record = widget_record(self.little, self.ps, w);
        self.head(b"WI\0\0", address, 0, 1, &record);
    }
}

pub fn synthetic(little: bool, ps: usize) -> Vec<u8> {
    synthetic_declarators(little, ps, false)
}
pub fn synthetic_declarators(little: bool, ps: usize, callable: bool) -> Vec<u8> {
    let mut s = Synth::new(little, ps);
    // W1: two records. Record 1 keeps the shared allocations but owns its own
    // one-slot table, proving the selected element's pointer is the one read.
    let first = W {
        label: 0x2000,
        totcol: 3,
        parts: 0x3000,
        raw: 0x2100,
        single: 0x5000,
        numbers: 0x2200,
        arr: [0x2300, 0x2400],
        blob: 0x2500,
        ibytes: 0x2600,
    };
    let second = W {
        totcol: 1,
        parts: 0x3200,
        ..first.clone()
    };
    let mut w1 = widget_record(s.little, ps, &first);
    w1.extend(widget_record(s.little, ps, &second));
    s.head(b"WI\0\0", 0x1000, 0, 2, &w1);
    // W1's owner DATA run: byte allocations, then the pointer tables. The
    // 0x3000 table is immediately followed by the 0x3200 DATA block, so any
    // over-count read would cross into adjacent DATA.
    s.head(
        b"DATA",
        0x2000,
        0,
        1,
        &[0x00, 0x41, 0x00, 0x42, 0x43, 0x00, 0x00, 0x44],
    );
    s.head(b"DATA", 0x2100, 0, 1, &[0xFF, 0x00, 0x01]);
    s.head(b"DATA", 0x2200, 0, 1, &[0u8; 16]);
    s.head(b"DATA", 0x2300, 0, 1, &[0x7A, 0x00, 0x7B, 0x00]);
    s.head(b"DATA", 0x2400, 0, 1, &[0x63, 0x64, 0x00, 0x65]);
    s.head(b"DATA", 0x2500, 0, 1, &[0u8; 16]);
    s.head(b"DATA", 0x2600, 0, 1, &[0x80, 0x00, 0x7F, 0xFF, 0x00]);
    s.head(
        b"DATA",
        0x3000,
        0,
        1,
        &table(s.little, ps, &[0x5000, 0, 0x6000]),
    );
    s.head(b"DATA", 0x3200, 0, 1, &table(s.little, ps, &[0x6000]));
    // ID-family targets for table entries.
    s.head(b"PA\0\0", 0x5000, 1, 1, &[0u8; 12]);
    s.head(b"PA\0\0", 0x6000, 1, 1, &[0u8; 12]);
    s.head(b"OB\0\0", 0x7000, 3, 1, &[0u8; 4]);
    // Two ID blocks sharing one address: an entry pointing here is ambiguous.
    s.head(b"PA\0\0", 0x8000, 1, 1, &[0u8; 12]);
    s.head(b"PA\0\0", 0x8000, 1, 1, &[0u8; 12]);
    // W2: hostile data/table addresses in an owner run with no DATA.
    s.widget(
        0x9000,
        &W {
            label: 0xDEAD01,
            parts: 0x3000,
            raw: 0x5000,
            ..Default::default()
        },
    );
    // W3: its own scope repeats one DATA address.
    s.widget(
        0x9500,
        &W {
            totcol: 1,
            parts: 0x9600,
            ..Default::default()
        },
    );
    s.head(b"DATA", 0x9600, 0, 1, &table(s.little, ps, &[0x5000]));
    s.head(b"DATA", 0x9600, 0, 1, &table(s.little, ps, &[0x6000]));
    // W4: dangling table entry at index 1.
    s.widget(
        0xA000,
        &W {
            totcol: 2,
            parts: 0xA100,
            ..Default::default()
        },
    );
    s.head(
        b"DATA",
        0xA100,
        0,
        1,
        &table(s.little, ps, &[0x5000, 0xDEAD]),
    );
    // W5: entry resolves to an ID of the wrong kind.
    s.widget(
        0xB000,
        &W {
            totcol: 1,
            parts: 0xB100,
            ..Default::default()
        },
    );
    s.head(b"DATA", 0xB100, 0, 1, &table(s.little, ps, &[0x7000]));
    // W6: entry address is a duplicate ID (ambiguous).
    s.widget(
        0xB500,
        &W {
            totcol: 1,
            parts: 0xB600,
            ..Default::default()
        },
    );
    s.head(b"DATA", 0xB600, 0, 1, &table(s.little, ps, &[0x8000]));
    // W7: null table and null byte pointer with a nonzero saved count.
    s.widget(
        0xC000,
        &W {
            totcol: 2,
            ..Default::default()
        },
    );
    let dna = dna_payload(s.little, ps, callable);
    s.head(b"DNA1", 0, 0, 1, &dna);
    s.head(b"ENDB", 0, 0, 0, &[]);
    s.out
}
