//! Lossless JPEG optimisation (jpegtran-style coefficient transcoding).
//!
//! The DCT coefficients are read with `jpeg_read_coefficients` and written
//! back with `jpeg_write_coefficients`, re-optimising the Huffman tables and
//! (optionally) converting to progressive with mozjpeg's scan optimisation.
//! The decoded pixels are bit-identical to the input.

use std::mem;
use std::os::raw::{c_int, c_ulong};
use std::ptr;

use mozjpeg_sys as ffi;

use crate::error::{guard, Error, Result};

const BUF_SIZE: usize = 64 * 1024;

#[repr(C)]
struct VecDest {
    iface: ffi::jpeg_destination_mgr,
    out: Vec<u8>,
    buf: Vec<u8>,
}

unsafe extern "C-unwind" fn init_destination(cinfo: &mut ffi::jpeg_compress_struct) {
    // SAFETY: `dest` points at the `iface` field of a live, repr(C) VecDest.
    let d = unsafe { &mut *(cinfo.dest as *mut VecDest) };
    d.iface.next_output_byte = d.buf.as_mut_ptr();
    d.iface.free_in_buffer = d.buf.len();
}

unsafe extern "C-unwind" fn empty_output_buffer(
    cinfo: &mut ffi::jpeg_compress_struct,
) -> ffi::boolean {
    // SAFETY: as above.
    let d = unsafe { &mut *(cinfo.dest as *mut VecDest) };
    d.out.extend_from_slice(&d.buf);
    d.iface.next_output_byte = d.buf.as_mut_ptr();
    d.iface.free_in_buffer = d.buf.len();
    1
}

unsafe extern "C-unwind" fn term_destination(cinfo: &mut ffi::jpeg_compress_struct) {
    // SAFETY: as above.
    let d = unsafe { &mut *(cinfo.dest as *mut VecDest) };
    let used = d.buf.len().saturating_sub(d.iface.free_in_buffer);
    d.out.extend_from_slice(&d.buf[..used]);
    d.iface.free_in_buffer = d.buf.len();
    d.iface.next_output_byte = d.buf.as_mut_ptr();
}

unsafe extern "C-unwind" fn error_exit(cinfo: &mut ffi::jpeg_common_struct) {
    let mut msg = format!("libjpeg error {}", unsafe { (*cinfo.err).msg_code });
    // SAFETY: err is the manager we installed; format_message writes at most
    // 80 bytes (JMSG_LENGTH_MAX) into the buffer. Its binding is declared with
    // `&[u8; 80]`, so we go through a raw-pointer cast like the mozjpeg crate.
    unsafe {
        if let Some(fmt) = (*cinfo.err).format_message {
            let mut buffer = [0u8; 80];
            let f: unsafe extern "C-unwind" fn(&mut ffi::jpeg_common_struct, *mut u8) =
                mem::transmute(fmt);
            f(cinfo, buffer.as_mut_ptr());
            let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
            if end > 0 {
                msg = String::from_utf8_lossy(&buffer[..end]).into_owned();
            }
        }
    }
    std::panic::resume_unwind(Box::new(msg));
}

unsafe extern "C-unwind" fn silence(_cinfo: &mut ffi::jpeg_common_struct, _level: c_int) {}

struct DecompressGuard(*mut ffi::jpeg_decompress_struct);
impl Drop for DecompressGuard {
    fn drop(&mut self) {
        // SAFETY: pointer to a created decompress struct that outlives us.
        unsafe { ffi::jpeg_destroy_decompress(&mut *self.0) }
    }
}

struct CompressGuard(*mut ffi::jpeg_compress_struct);
impl Drop for CompressGuard {
    fn drop(&mut self) {
        // SAFETY: pointer to a created compress struct that outlives us.
        unsafe { ffi::jpeg_destroy_compress(&mut *self.0) }
    }
}

/// Losslessly re-encode a JPEG's entropy coding. Metadata segments are not
/// copied; the caller re-attaches them.
pub(crate) fn optimize(input: &[u8], progressive: bool) -> Result<Vec<u8>> {
    if input.len() < 4 {
        return Err(Error::decode("JPEG", "file is too small"));
    }
    guard("JPEG", false, || transcode(input, progressive))
}

fn transcode(input: &[u8], progressive: bool) -> Result<Vec<u8>> {
    // Declaration order matters: guards are dropped (destroying the libjpeg
    // objects) before the boxes they point into.
    // SAFETY: all-zero is the documented initial state for these C structs
    // (they are fully initialised by jpeg_std_error / jpeg_create_*).
    let mut err: Box<ffi::jpeg_error_mgr> = Box::new(unsafe { mem::zeroed() });
    let mut src: Box<ffi::jpeg_decompress_struct> = Box::new(unsafe { mem::zeroed() });
    let mut dst: Box<ffi::jpeg_compress_struct> = Box::new(unsafe { mem::zeroed() });
    let mut dest = Box::new(VecDest {
        // SAFETY: plain C struct of pointers/integers/options; zero is valid.
        iface: unsafe { mem::zeroed() },
        out: Vec::with_capacity(input.len()),
        buf: vec![0u8; BUF_SIZE],
    });
    dest.iface.init_destination = Some(init_destination);
    dest.iface.empty_output_buffer = Some(empty_output_buffer);
    dest.iface.term_destination = Some(term_destination);

    // SAFETY: standard libjpeg transcoding sequence (see jpegtran.c). All
    // pointers stay valid for the duration: the boxes outlive the guards,
    // `input` outlives the decompressor. Fatal errors unwind via error_exit
    // and are caught by `guard`; the guards then release libjpeg memory.
    unsafe {
        ffi::jpeg_std_error(&mut err);
        err.error_exit = Some(error_exit);
        err.emit_message = Some(silence);

        src.common.err = &mut *err;
        ffi::jpeg_create_decompress(&mut *src);
        let _src_guard = DecompressGuard(&mut *src);
        dst.common.err = &mut *err;
        ffi::jpeg_create_compress(&mut *dst);
        let _dst_guard = CompressGuard(&mut *dst);

        ffi::jpeg_mem_src(&mut src, input.as_ptr(), input.len() as c_ulong);
        ffi::jpeg_read_header(&mut src, 1);
        let coefs = ffi::jpeg_read_coefficients(&mut src);
        if coefs.is_null() {
            return Err(Error::decode("JPEG", "could not read DCT coefficients"));
        }
        ffi::jpeg_copy_critical_parameters(&src, &mut dst);
        dst.optimize_coding = 1;
        if progressive {
            ffi::jpeg_c_set_bool_param(&mut dst, ffi::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS, 1);
            ffi::jpeg_simple_progression(&mut dst);
        } else {
            ffi::jpeg_c_set_bool_param(&mut dst, ffi::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS, 0);
            dst.num_scans = 0;
            dst.scan_info = ptr::null();
        }
        dst.dest = &mut dest.iface;
        ffi::jpeg_write_coefficients(&mut dst, coefs);
        ffi::jpeg_finish_compress(&mut dst);
        ffi::jpeg_finish_decompress(&mut src);
    }
    Ok(mem::take(&mut dest.out))
}
