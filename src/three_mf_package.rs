//! Restricted 3MF Core geometry importer: ZIP and OPC root relationships.
//! No extraction to disk, external relationships, ZIP64, or encrypted parts.
use crate::engine::cancelled;
use crate::stl_export::crc32;
use flate2::{Decompress, FlushDecompress, Status};
use std::collections::BTreeMap;

const MAX_PARTS: usize = 4096;
const MAX_PART: usize = 64 * 1024 * 1024;
const BAD: &str = "3MF の ZIP 構造が不正か未対応です。";

fn word(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes.get(at..at + 2).ok_or(BAD)?.try_into().map_err(|_| BAD)?,
    ))
}

fn dword(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes.get(at..at + 4).ok_or(BAD)?.try_into().map_err(|_| BAD)?,
    ))
}

#[derive(Clone, Copy)]
struct Entry {
    flags: u16,
    method: u16,
    crc: u32,
    packed: usize,
    unpacked: usize,
    local: usize,
}

pub(crate) struct Package<'a> {
    bytes: &'a [u8],
    entries: BTreeMap<&'a str, Entry>,
    directory: usize,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.starts_with('/')
        && !name.contains(['\\', ':', '?', '#', '%', '\0'])
        && name.trim_end_matches('/').split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".."
        })
}

impl<'a> Package<'a> {
    pub(crate) fn new(bytes: &'a [u8], stop: &dyn Fn() -> bool) -> Result<Self, String> {
        cancelled(stop)?;
        if bytes.len() > MAX_PART || bytes.len() < 22 {
            return Err(BAD.into());
        }
        // EOCD must end at EOF (including the declared archive comment).
        let start = bytes.len().saturating_sub(22 + 65_535);
        let end = (start..=bytes.len() - 22)
            .rev()
            .find(|&i| {
                bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]
                    && word(bytes, i + 20).ok().is_some_and(|n| {
                        i + 22 + usize::from(n) == bytes.len()
                    })
            })
            .ok_or(BAD)?;
        let count = usize::from(word(bytes, end + 10)?);
        let directory = dword(bytes, end + 16)? as usize;
        let size = dword(bytes, end + 12)? as usize;
        if word(bytes, end + 4)? != 0
            || word(bytes, end + 6)? != 0
            || usize::from(word(bytes, end + 8)?) != count
            || !(1..=MAX_PARTS).contains(&count)
            || directory.checked_add(size) != Some(end)
        {
            return Err(BAD.into());
        }
        let mut entries = BTreeMap::new();
        let mut at = directory;
        for _ in 0..count {
            cancelled(stop)?;
            if at.checked_add(46).is_none_or(|i| i > end)
                || dword(bytes, at)? != 0x0201_4b50
            {
                return Err(BAD.into());
            }
            let name_len = usize::from(word(bytes, at + 28)?);
            let extra_len = usize::from(word(bytes, at + 30)?);
            let comment_len = usize::from(word(bytes, at + 32)?);
            let name_start = at + 46;
            let name_end = name_start + name_len;
            let next = name_end + extra_len + comment_len;
            if next > end || word(bytes, at + 34)? != 0 {
                return Err(BAD.into());
            }
            let name = std::str::from_utf8(&bytes[name_start..name_end]).map_err(|_| BAD)?;
            if !valid_name(name) {
                return Err(BAD.into());
            }
            let entry = Entry {
                flags: word(bytes, at + 8)?,
                method: word(bytes, at + 10)?,
                crc: dword(bytes, at + 16)?,
                packed: dword(bytes, at + 20)? as usize,
                unpacked: dword(bytes, at + 24)? as usize,
                local: dword(bytes, at + 42)? as usize,
            };
            // Ignore compression on parts we do not read, but reject duplicate paths.
            if entry.local >= directory || entries.insert(name, entry).is_some() {
                return Err(BAD.into());
            }
            at = next;
        }
        if at != end {
            return Err(BAD.into());
        }
        Ok(Self {
            bytes,
            entries,
            directory,
        })
    }

    pub(crate) fn read(
        &self,
        name: &str,
        limit: usize,
        stop: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, String> {
        cancelled(stop)?;
        let e = self.entries.get(name).ok_or("3MF に必要な部品がありません。")?;
        let bytes = self.bytes;
        let at = e.local;
        // Only normal deflate flags (bits 1,2), data descriptor (3), UTF-8 (11).
        if e.flags & !0x080e != 0
            || !matches!(e.method, 0 | 8)
            || e.unpacked > limit.min(MAX_PART)
            || e.packed > MAX_PART
            || at.checked_add(30).is_none_or(|i| i > self.directory)
            || dword(bytes, at)? != 0x0403_4b50
            || word(bytes, at + 6)? != e.flags
            || word(bytes, at + 8)? != e.method
        {
            return Err(BAD.into());
        }
        if e.flags & 8 == 0
            && (dword(bytes, at + 14)? != e.crc
                || dword(bytes, at + 18)? as usize != e.packed
                || dword(bytes, at + 22)? as usize != e.unpacked)
        {
            return Err(BAD.into());
        }
        let name_start = at + 30;
        let name_end = name_start + usize::from(word(bytes, at + 26)?);
        let data_start = name_end + usize::from(word(bytes, at + 28)?);
        let data_end = data_start.checked_add(e.packed).ok_or(BAD)?;
        if data_end > self.directory
            || bytes.get(name_start..name_end) != Some(name.as_bytes())
        {
            return Err(BAD.into());
        }
        let data = &bytes[data_start..data_end];
        let out = match e.method {
            0 if e.packed == e.unpacked => data.to_vec(),
            8 => inflate(data, e.unpacked, stop)?,
            _ => return Err(BAD.into()),
        };
        cancelled(stop)?;
        if out.len() != e.unpacked || crc32(&out) != e.crc {
            return Err("3MF の展開サイズまたは CRC が一致しません。".into());
        }
        Ok(out)
    }
}

fn inflate(data: &[u8], expected: usize, stop: &dyn Fn() -> bool) -> Result<Vec<u8>, String> {
    let mut decoder = Decompress::new(false);
    let mut out = Vec::with_capacity(expected.min(64 * 1024));
    let mut chunk = [0u8; 32 * 1024];
    loop {
        cancelled(stop)?;
        let before_in = decoder.total_in() as usize;
        let before_out = decoder.total_out() as usize;
        let status = decoder
            .decompress(&data[before_in..], &mut chunk, FlushDecompress::None)
            .map_err(|_| BAD)?;
        let after_in = decoder.total_in() as usize;
        let after_out = decoder.total_out() as usize;
        let count = after_out - before_out;
        if after_out > expected {
            return Err("3MF の展開サイズが申告値を超えました。".into());
        }
        out.extend_from_slice(&chunk[..count]);
        if status == Status::StreamEnd {
            if after_in != data.len() || after_out != expected {
                return Err(BAD.into());
            }
            return Ok(out);
        }
        if after_in == before_in && count == 0 {
            return Err(BAD.into());
        }
    }
}
