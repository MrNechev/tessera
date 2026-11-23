//! A streaming ZIP writer (non-seekable output), laid out like Go's `archive/zip`: entries
//! use data descriptors, deflate or store, an extended-timestamp field when a modification
//! time is known, and ZIP64 records for entries/archives beyond 4 GiB.

use std::io::{self, Read, Write};
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local, Timelike};
use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Store,
    Deflate,
}

struct Entry {
    name: Vec<u8>,
    flags: u16,
    method: u16,
    mod_time: u16,
    mod_date: u16,
    crc: u32,
    csize: u64,
    usize: u64,
    offset: u64,
    extra: Vec<u8>,
}

struct Counter<W> {
    w: W,
    n: u64,
}

impl<W: Write> Write for Counter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.w.write(buf)?;
        self.n += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

/// Writes a ZIP archive to any `Write`.
pub struct ZipStream<W: Write> {
    w: Counter<W>,
    entries: Vec<Entry>,
}

const U32MAX: u64 = u32::MAX as u64;

fn dos_time(t: SystemTime) -> (u16, u16) {
    let t: DateTime<Local> = t.into();
    if t.year() < 1980 {
        return (0, (1 << 5) | 1);
    }
    let date = (((t.year() - 1980) as u16) << 9) | ((t.month() as u16) << 5) | t.day() as u16;
    let time = ((t.hour() as u16) << 11) | ((t.minute() as u16) << 5) | (t.second() as u16 / 2);
    (time, date)
}

impl<W: Write> ZipStream<W> {
    pub fn new(w: W) -> Self {
        ZipStream {
            w: Counter { w, n: 0 },
            entries: Vec::new(),
        }
    }

    /// Adds one file, streaming its content from `r`.
    pub fn add(
        &mut self,
        name: &str,
        method: Method,
        modified: Option<SystemTime>,
        r: &mut dyn Read,
    ) -> io::Result<()> {
        let mut flags: u16 = 0x8;
        if !name.is_ascii() {
            flags |= 0x800;
        }
        let (mod_time, mod_date, extra) = match modified {
            Some(t) => {
                let (tm, dt) = dos_time(t);
                let unix = t
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs() as u32)
                    .unwrap_or(0);
                let mut e = Vec::with_capacity(9);
                e.extend_from_slice(&0x5455u16.to_le_bytes());
                e.extend_from_slice(&5u16.to_le_bytes());
                e.push(1);
                e.extend_from_slice(&unix.to_le_bytes());
                (tm, dt, e)
            }
            None => (0, 0, Vec::new()),
        };
        let method_id: u16 = match method {
            Method::Store => 0,
            Method::Deflate => 8,
        };
        let offset = self.w.n;
        let mut h = Vec::with_capacity(30 + name.len() + extra.len());
        h.extend_from_slice(&0x04034b50u32.to_le_bytes());
        h.extend_from_slice(&20u16.to_le_bytes());
        h.extend_from_slice(&flags.to_le_bytes());
        h.extend_from_slice(&method_id.to_le_bytes());
        h.extend_from_slice(&mod_time.to_le_bytes());
        h.extend_from_slice(&mod_date.to_le_bytes());
        h.extend_from_slice(&[0u8; 12]); // crc, sizes: in the data descriptor
        h.extend_from_slice(&(name.len() as u16).to_le_bytes());
        h.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        h.extend_from_slice(name.as_bytes());
        h.extend_from_slice(&extra);
        self.w.write_all(&h)?;

        let start = self.w.n;
        let mut crc = Crc::new();
        let mut usize: u64 = 0;
        let mut buf = vec![0u8; 256 << 10];
        match method {
            Method::Store => loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                crc.update(&buf[..n]);
                usize += n as u64;
                self.w.write_all(&buf[..n])?;
            },
            Method::Deflate => {
                let mut enc = DeflateEncoder::new(&mut self.w, Compression::default());
                loop {
                    let n = r.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    crc.update(&buf[..n]);
                    usize += n as u64;
                    enc.write_all(&buf[..n])?;
                }
                enc.finish()?;
            }
        }
        let csize = self.w.n - start;
        let zip64 = csize >= U32MAX || usize >= U32MAX;
        let mut dd = Vec::with_capacity(24);
        dd.extend_from_slice(&0x08074b50u32.to_le_bytes());
        dd.extend_from_slice(&crc.sum().to_le_bytes());
        if zip64 {
            dd.extend_from_slice(&csize.to_le_bytes());
            dd.extend_from_slice(&usize.to_le_bytes());
        } else {
            dd.extend_from_slice(&(csize as u32).to_le_bytes());
            dd.extend_from_slice(&(usize as u32).to_le_bytes());
        }
        self.w.write_all(&dd)?;
        self.entries.push(Entry {
            name: name.as_bytes().to_vec(),
            flags,
            method: method_id,
            mod_time,
            mod_date,
            crc: crc.sum(),
            csize,
            usize,
            offset,
            extra,
        });
        Ok(())
    }

    /// Adds a file from memory.
    pub fn add_bytes(&mut self, name: &str, method: Method, data: &[u8]) -> io::Result<()> {
        let mut r = data;
        self.add(name, method, None, &mut r)
    }

    /// Writes the central directory and returns the inner writer.
    pub fn finish(mut self) -> io::Result<W> {
        let cd_start = self.w.n;
        for e in &self.entries {
            let zip64 = e.csize >= U32MAX || e.usize >= U32MAX || e.offset >= U32MAX;
            let mut extra = e.extra.clone();
            let mut reader_version: u16 = 20;
            let (mut cs, mut us, mut off) = (e.csize as u32, e.usize as u32, e.offset as u32);
            if zip64 {
                reader_version = 45;
                let mut z = Vec::new();
                z.extend_from_slice(&e.usize.to_le_bytes());
                z.extend_from_slice(&e.csize.to_le_bytes());
                cs = u32::MAX;
                us = u32::MAX;
                if e.offset >= U32MAX {
                    z.extend_from_slice(&e.offset.to_le_bytes());
                    off = u32::MAX;
                }
                let mut zx = Vec::new();
                zx.extend_from_slice(&1u16.to_le_bytes());
                zx.extend_from_slice(&(z.len() as u16).to_le_bytes());
                zx.extend_from_slice(&z);
                extra.extend_from_slice(&zx);
            }
            let mut h = Vec::with_capacity(46 + e.name.len() + extra.len());
            h.extend_from_slice(&0x02014b50u32.to_le_bytes());
            h.extend_from_slice(&20u16.to_le_bytes()); // creator version
            h.extend_from_slice(&reader_version.to_le_bytes());
            h.extend_from_slice(&e.flags.to_le_bytes());
            h.extend_from_slice(&e.method.to_le_bytes());
            h.extend_from_slice(&e.mod_time.to_le_bytes());
            h.extend_from_slice(&e.mod_date.to_le_bytes());
            h.extend_from_slice(&e.crc.to_le_bytes());
            h.extend_from_slice(&cs.to_le_bytes());
            h.extend_from_slice(&us.to_le_bytes());
            h.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            h.extend_from_slice(&(extra.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // comment
            h.extend_from_slice(&0u16.to_le_bytes()); // disk
            h.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            h.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            h.extend_from_slice(&off.to_le_bytes());
            h.extend_from_slice(&e.name);
            h.extend_from_slice(&extra);
            self.w.write_all(&h)?;
        }
        let cd_end = self.w.n;
        let records = self.entries.len() as u64;
        let size = cd_end - cd_start;
        let offset = cd_start;
        let (mut rec16, mut size32, mut off32) = (records, size, offset);
        if records >= 0xffff || size >= U32MAX || offset >= U32MAX {
            // zip64 end of central directory record + locator
            let mut b = Vec::new();
            b.extend_from_slice(&0x06064b50u32.to_le_bytes());
            b.extend_from_slice(&44u64.to_le_bytes());
            b.extend_from_slice(&45u16.to_le_bytes());
            b.extend_from_slice(&45u16.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&records.to_le_bytes());
            b.extend_from_slice(&records.to_le_bytes());
            b.extend_from_slice(&size.to_le_bytes());
            b.extend_from_slice(&offset.to_le_bytes());
            b.extend_from_slice(&0x07064b50u32.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&cd_end.to_le_bytes());
            b.extend_from_slice(&1u32.to_le_bytes());
            self.w.write_all(&b)?;
            rec16 = 0xffff;
            size32 = U32MAX;
            off32 = U32MAX;
        }
        let mut b = Vec::with_capacity(22);
        b.extend_from_slice(&0x06054b50u32.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&(rec16.min(0xffff) as u16).to_le_bytes());
        b.extend_from_slice(&(rec16.min(0xffff) as u16).to_le_bytes());
        b.extend_from_slice(&(size32.min(U32MAX) as u32).to_le_bytes());
        b.extend_from_slice(&(off32.min(U32MAX) as u32).to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        self.w.write_all(&b)?;
        self.w.flush()?;
        Ok(self.w.w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_by_unzip() {
        let mut z = ZipStream::new(Vec::new());
        z.add_bytes("a/hello.txt", Method::Deflate, b"hello hello hello")
            .unwrap();
        z.add(
            "a/raw.bin",
            Method::Store,
            Some(SystemTime::now()),
            &mut &[1u8, 2, 3][..],
        )
        .unwrap();
        z.add_bytes("ü.txt", Method::Deflate, b"").unwrap();
        let bytes = z.finish().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.zip");
        std::fs::write(&p, &bytes).unwrap();
        // verify with Python's zipfile if available (no extra crate needed)
        if crate::util::look_path("python3").is_some() {
            let out = std::process::Command::new("python3")
                .arg("-I")
                .arg("-c")
                .arg("import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; print(sorted(z.namelist())); print(z.read('a/hello.txt').decode())")
                .arg(&p)
                .output()
                .unwrap();
            let s = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(s.contains("'a/hello.txt', 'a/raw.bin', 'ü.txt'"), "{s}");
            assert!(s.contains("hello hello hello"), "{s}");
        }
    }
}
