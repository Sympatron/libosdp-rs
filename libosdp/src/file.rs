//
// Copyright (c) 2023-2024 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! OSDP provides a means to send files from CP to a Peripheral Device (PD).
//! This module adds the required components to achieve this effect.

use crate::OsdpError;
use std::{ffi::c_void, fs::File, path::PathBuf};

#[cfg(not(target_os = "windows"))]
use std::os::unix::prelude::FileExt;
#[cfg(target_os = "windows")]
use std::os::windows::fs::FileExt;

type Result<T> = std::result::Result<T, OsdpError>;

trait OffsetRead {
    fn pread(&self, buf: &mut [u8], offset: u64) -> std::io::Result<usize>;
    fn pwrite(&self, buf: &[u8], offset: u64) -> std::io::Result<usize>;
}

impl OffsetRead for std::fs::File {
    #[inline(always)]
    fn pread(&self, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
        #[cfg(not(target_os = "windows"))]
        return self.read_at(buf, offset);

        #[cfg(target_os = "windows")]
        return self.seek_read(buf, offset);
    }

    #[inline(always)]
    fn pwrite(&self, buf: &[u8], offset: u64) -> std::io::Result<usize> {
        #[cfg(not(target_os = "windows"))]
        return self.write_at(buf, offset);

        #[cfg(target_os = "windows")]
        return self.seek_write(buf, offset);
    }
}

/// OSDP file transfer context
#[derive(Debug)]
pub struct OsdpFile {
    id: i32,
    path: PathBuf,
    file: Option<File>,
    size: usize,
}

unsafe extern "C" fn raw_file_open(data: *mut c_void, file_id: i32, size: *mut i32) -> i32 {
    let ctx = &mut *(data as *mut OsdpFile);
    if ctx.file.is_some() || file_id != ctx.id {
        return -1;
    }
    let file = match File::open(ctx.path.as_os_str()) {
        Ok(f) => f,
        Err(_) => return -1,
    };
    ctx.size = file.metadata().unwrap().len() as usize;
    ctx.file = Some(file);
    unsafe {
        *size = ctx.size as i32;
    }
    0
}

unsafe extern "C" fn raw_file_read(
    data: *mut c_void,
    buf: *mut c_void,
    size: i32,
    offset: i32,
) -> i32 {
    let ctx = &mut *(data as *mut OsdpFile);
    if ctx.file.is_none() {
        return -1;
    }
    let file = ctx.file.as_ref().unwrap();
    let mut read_buf = vec![0u8; size as usize];
    let len = match file.pread(&mut read_buf, offset as u64) {
        Ok(len) => len as i32,
        Err(_) => -1,
    };
    std::ptr::copy_nonoverlapping(read_buf.as_mut_ptr(), buf as *mut u8, len as usize);
    len
}

unsafe extern "C" fn raw_file_write(
    data: *mut c_void,
    buf: *const c_void,
    size: i32,
    offset: i32,
) -> i32 {
    let ctx = &mut *(data as *mut OsdpFile);
    if ctx.file.is_none() {
        return -1;
    }
    let mut write_buf = vec![0u8; size as usize];
    std::ptr::copy_nonoverlapping(buf as *mut u8, write_buf.as_mut_ptr(), size as usize);
    let file = ctx.file.as_ref().unwrap();
    match file.pwrite(&write_buf, offset as u64) {
        Ok(len) => len as i32,
        Err(_) => -1,
    }
}

unsafe extern "C" fn raw_file_close(data: *mut c_void) -> i32 {
    let ctx = &mut *(data as *mut OsdpFile);
    if ctx.file.is_none() {
        return -1;
    }
    let _ = ctx.file.take().unwrap();
    0
}

impl OsdpFile {
    /// Create a new file transfer context for a given ID and path.
    ///
    /// # Arguments
    ///
    /// * `id` - An ID to associate to file. This ID must be pre-shared between
    ///   CP and PD.
    /// * `path` - Path to file to read from (CP) or write to (PD).
    pub fn new(id: i32, path: PathBuf) -> Self {
        Self {
            id,
            path,
            file: None,
            size: 0,
        }
    }

    /// For internal use by {cp,pd}.register_file() methods.
    pub fn get_ops_struct(&mut self) -> libosdp_sys::osdp_file_ops {
        libosdp_sys::osdp_file_ops {
            arg: self as *mut _ as *mut c_void,
            open: Some(raw_file_open),
            read: Some(raw_file_read),
            write: Some(raw_file_write),
            close: Some(raw_file_close),
        }
    }
}

/// A OSDP File transfer Ops trait.
pub trait OsdpFileOps {
    /// Method used to register a file transfer operation. The `pd` must be
    /// set to zero for PeripheralDevice
    ///
    /// TODO: Remove the `pd` arg for PD mode.
    fn register_file(&mut self, pd: i32, fm: &mut OsdpFile) -> Result<()>;

    /// Method used check the status of an ongoing transfer. The `pd` must be
    /// set to zero for PeripheralDevice
    ///
    /// TODO: Remove the `pd` arg for PD mode.
    fn get_file_transfer_status(&self, pd: i32) -> Result<(i32, i32)>;
}

macro_rules! impl_osdp_file_ops_for {
    ($($t:ty),+ $(,)?) => ($(
        impl OsdpFileOps for $t {
            fn register_file(&mut self, pd: i32, fm: &mut OsdpFile) -> Result<()> {
                let mut ops = fm.get_ops_struct();
                let rc = unsafe {
                    libosdp_sys::osdp_file_register_ops(self.ctx, pd, &mut ops as *mut libosdp_sys::osdp_file_ops)
                };
                if rc < 0 {
                    Err(OsdpError::FileTransfer("ops register"))
                } else {
                    Ok(())
                }
            }

            fn get_file_transfer_status(&self, pd: i32) -> Result<(i32, i32)> {
                let mut size: i32 = 0;
                let mut offset: i32 = 0;
                let rc = unsafe {
                    libosdp_sys::osdp_get_file_tx_status(
                        self.ctx,
                        pd,
                        &mut size as *mut i32,
                        &mut offset as *mut i32,
                    )
                };
                if rc < 0 {
                    Err(OsdpError::FileTransfer("transfer status query"))
                } else {
                    Ok((size, offset))
                }
            }
        }
    )+)
}
pub(crate) use impl_osdp_file_ops_for;
