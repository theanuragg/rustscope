//! # pprof protobuf export
//!
//! Exports a `ProfileSession` as a gzip-compressed pprof protobuf profile.
//! Compatible with: `go tool pprof`, Pyroscope, Grafana Phlare, Polar Signals,
//! Datadog Continuous Profiler.
//!
//! ## Usage
//! ```rust,ignore
//! use rustscope::features::pprof_export;
//! let session = rustscope::Profiler::collect();
//! pprof_export::save(&session, "profile.pb.gz").unwrap();
//! ```
//!
//! Then:
//! ```sh
//! go tool pprof -http=:8080 profile.pb.gz
//! # or
//! go tool pprof profile.pb.gz
//! # (top) → top 10 by CPU, (web) → flame graph in browser
//! ```
//!
//! ## Wire format
//! Implements the pprof proto3 spec (profile.proto from github.com/google/pprof).
//! Hand-rolled varint/length-delimited encoding — no `prost` dependency.
//! GZIP uses DEFLATE stored blocks (no compression) for maximum compatibility.

use std::collections::HashMap;
use std::io::{self, Write};

use crate::output::schema::ProfileSession;

// ─── protobuf wire encoding ───────────────────────────────────────────────────

fn varint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 { buf.push(b); return; }
        buf.push(b | 0x80);
    }
}

fn field_varint(buf: &mut Vec<u8>, field: u32, v: u64) {
    varint(buf, (field as u64) << 3);   // wire type 0
    varint(buf, v);
}

fn field_bytes(buf: &mut Vec<u8>, field: u32, data: &[u8]) {
    varint(buf, ((field as u64) << 3) | 2); // wire type 2
    varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

fn field_str(buf: &mut Vec<u8>, field: u32, s: &str) {
    field_bytes(buf, field, s.as_bytes());
}

fn submsg(buf: &mut Vec<u8>, field: u32, msg: &[u8]) {
    field_bytes(buf, field, msg);
}

// ─── string table ─────────────────────────────────────────────────────────────

struct StringTable {
    strings: Vec<String>,
    index: HashMap<String, usize>,
}

impl StringTable {
    fn new() -> Self {
        let mut st = Self { strings: Vec::new(), index: HashMap::new() };
        st.intern("");  // index 0 must be empty string per pprof spec
        st
    }
    fn intern(&mut self, s: &str) -> i64 {
        if let Some(&i) = self.index.get(s) { return i as i64; }
        let i = self.strings.len();
        self.strings.push(s.to_owned());
        self.index.insert(s.to_owned(), i);
        i as i64
    }
}

// ─── profile encoder ─────────────────────────────────────────────────────────

/// Encode a `ProfileSession` as raw pprof protobuf bytes (uncompressed).
pub fn encode_profile(session: &ProfileSession) -> Vec<u8> {
    let mut st = StringTable::new();
    let mut buf = Vec::<u8>::new();

    // SampleType[0]: cpu / nanoseconds
    {
        let t = st.intern("cpu");
        let u = st.intern("nanoseconds");
        let mut m = Vec::new();
        field_varint(&mut m, 1, t as u64);
        field_varint(&mut m, 2, u as u64);
        submsg(&mut buf, 1, &m);
    }
    // SampleType[1]: alloc_space / bytes
    {
        let t = st.intern("alloc_space");
        let u = st.intern("bytes");
        let mut m = Vec::new();
        field_varint(&mut m, 1, t as u64);
        field_varint(&mut m, 2, u as u64);
        submsg(&mut buf, 1, &m);
    }

    // Build ID maps (1-based)
    let mut fn_ids: HashMap<String, u64> = HashMap::new();
    let mut loc_ids: HashMap<String, u64> = HashMap::new();
    let mut next_id = 1u64;

    for f in &session.functions {
        let key = format!("{}::{}", f.module_path, f.name);
        fn_ids.entry(key.clone()).or_insert_with(|| { let id = next_id; next_id += 1; id });
        loc_ids.entry(key).or_insert_with(|| { let id = next_id; next_id += 1; id });
    }

    // Sample per function (field 2)
    for f in &session.functions {
        let key = format!("{}::{}", f.module_path, f.name);
        if let Some(&lid) = loc_ids.get(&key) {
            let alloc = f.memory.as_ref().map(|m| m.total_alloc_bytes).unwrap_or(0);

            let mut s = Vec::new();
            // location_id list — encode as repeated varint (packed)
            let mut loc_packed = Vec::new();
            varint(&mut loc_packed, lid);
            field_bytes(&mut s, 1, &loc_packed);
            // value[0] = cpu ns
            field_varint(&mut s, 2, f.timing.total_ns);
            // value[1] = alloc bytes
            field_varint(&mut s, 2, alloc);
            submsg(&mut buf, 2, &s);
        }
    }

    // Location per function (field 4)
    for f in &session.functions {
        let key = format!("{}::{}", f.module_path, f.name);
        if let (Some(&lid), Some(&fid)) = (loc_ids.get(&key), fn_ids.get(&key)) {
            let mut loc = Vec::new();
            field_varint(&mut loc, 1, lid);   // id
            // line sub-message (field 3)
            let mut line_msg = Vec::new();
            field_varint(&mut line_msg, 1, fid);           // function_id
            field_varint(&mut line_msg, 2, f.line as u64); // line
            submsg(&mut loc, 3, &line_msg);
            submsg(&mut buf, 4, &loc);
        }
    }

    // Function per unique function (field 5)
    for f in &session.functions {
        let key = format!("{}::{}", f.module_path, f.name);
        if let Some(&fid) = fn_ids.get(&key) {
            let name_i   = st.intern(&f.name);
            let file_i   = st.intern(&f.file);
            let sysname_i = st.intern(&key);  // fully qualified name
            let mut func = Vec::new();
            field_varint(&mut func, 1, fid);              // id
            field_varint(&mut func, 2, name_i as u64);    // name
            field_varint(&mut func, 3, sysname_i as u64); // system_name (full path)
            field_varint(&mut func, 4, file_i as u64);    // filename
            field_varint(&mut func, 5, f.line as u64);    // start_line
            submsg(&mut buf, 5, &func);
        }
    }

    // String table (field 6) — must be last
    for s in &st.strings {
        field_str(&mut buf, 6, s);
    }

    // duration_nanos (field 10)
    field_varint(&mut buf, 10, session.session_duration_ns);
    // time_nanos (field 9)
    field_varint(&mut buf, 9, session.started_at_unix_secs.saturating_mul(1_000_000_000));

    buf
}

/// Export as gzip-compressed pprof to `path`.
pub fn save(session: &ProfileSession, path: &str) -> io::Result<()> {
    let pb = encode_profile(session);
    let gz = gzip_compress(&pb)?;
    std::fs::write(path, &gz)?;
    println!("[rustscope/pprof] {path}: {} functions → {} bytes (gzipped)",
        session.functions.len(), gz.len());
    Ok(())
}

// ─── minimal gzip / DEFLATE stored-block encoder ─────────────────────────────
// RFC 1952 (GZIP) + RFC 1951 (DEFLATE, stored blocks = no compression)
// go tool pprof accepts this. For smaller files use the `flate2` crate.

fn gzip_compress(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() + 32);

    // GZIP header: ID1 ID2 CM FLG MTIME(4) XFL OS
    out.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, 0xff]);

    // DEFLATE: one or more stored blocks (BTYPE=00, max 65535 bytes each)
    let mut offset = 0;
    while offset < data.len() || data.is_empty() {
        let chunk_end = (offset + 65535).min(data.len());
        let chunk = &data[offset..chunk_end];
        let is_last = chunk_end >= data.len();
        let bfinal = if is_last { 1u8 } else { 0u8 };
        let len = chunk.len() as u16;
        let nlen = !len;

        out.push(bfinal);                       // BFINAL | (BTYPE=00)<<1
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(chunk);

        if is_last { break; }
        offset = chunk_end;
    }

    // CRC32 and ISIZE
    let crc = crc32(data);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());

    Ok(out)
}

fn crc32(data: &[u8]) -> u32 {
    let table = crc32_table();
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

const fn crc32_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            j += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}
