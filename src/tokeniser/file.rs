use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use std::io::Read;

pub fn read_i16<T: Read>(f: &mut T) -> Result<i16> {
    read_u16(f).map(|val| val as i16)
}
pub fn read_u16<T: Read>(f: &mut T) -> Result<u16> {
    let mut buffer = [0; 2];
    match f.read_exact(&mut buffer) {
        Ok(()) => Ok(u16::from_le_bytes(buffer)),
        _ => bail!("IO error"),
    }
}
pub fn read_u32<T: Read>(f: &mut T) -> Result<u32> {
    let mut buffer = [0; 4];
    match f.read_exact(&mut buffer) {
        Ok(()) => Ok(u32::from_le_bytes(buffer)),
        _ => bail!("IO error"),
    }
}

unsafe fn as_byte_slice_mut<T>(slice: &mut [T]) -> &mut [u8] {
    std::slice::from_raw_parts_mut(
        slice.as_mut_ptr() as *mut u8,
        slice.len() * std::mem::size_of::<T>(),
    )
}

pub fn read_i16_buffer<T: Read>(f: &mut T, dst: &mut [i16]) -> Result<()> {
    let dst_b = unsafe { as_byte_slice_mut(dst) };
    f.read_exact(dst_b).context("IO error")?;

    for val in dst.iter_mut() {
        *val = i16::from_le(*val);
    }

    Ok(())
}

fn trim_at_null(mystr: &[u8]) -> &[u8] {
    let mut nullpos = 0usize;
    while nullpos < mystr.len() && mystr[nullpos] != 0 {
        nullpos += 1
    }
    &mystr[..nullpos]
}

pub fn read_nstr<T: Read>(f: &mut T, n: usize) -> Result<String> {
    let mut buf = vec![0u8; n];

    match f.read_exact(&mut buf) {
        Ok(_) => {
            let mystr = std::str::from_utf8(trim_at_null(&buf));

            if let Ok(mystr) = mystr {
                Ok(mystr.to_string())
            } else {
                bail!("Decoding error")
            }
        }
        _ => bail!("IO error"),
    }
}
pub fn read_str_buffer(buf: &[u8]) -> Result<String> {
    let mystr = std::str::from_utf8(trim_at_null(buf));

    if let Ok(mystr) = mystr {
        Ok(mystr.to_string())
    } else {
        bail!("UTF-8 decoding error")
    }
}

// this is way, WAY faster than seeking 4 bytes forward explicitly.
pub fn seek_rel_4<T: Read>(f: &mut T) -> Result<()> {
    let mut bogus = [0u8; 4];
    match f.read_exact(&mut bogus) {
        Ok(_) => Ok(()),
        _ => bail!("IO error"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn null_padded_string_decode() {
        let vec = vec![0x20u8, 0x00u8, 0x00u8];
        assert_eq!(super::read_str_buffer(&vec).unwrap(), (" ".to_string()));
    }
    #[test]
    fn null_comma_strings_decode_first_only() {
        let vec = vec![0x20u8, 0x00u8, 0x20u8];
        assert_eq!(super::read_str_buffer(&vec).unwrap(), (" ".to_string()));
    }
    #[test]
    fn read_i16_buffer() {
        let input = &[0x12, 0x34, 0x56, 0x78];
        let mut out = [0i16, 2];
        super::read_i16_buffer(&mut &input[..], &mut out).unwrap();
        assert_eq!(out, [0x3412, 0x7856]);
    }
}