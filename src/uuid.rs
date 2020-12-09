#![feature(const_fn)]

use thiserror::Error;

pub type Uuid16 = u16;
pub type Uuid32 = u32;
pub type Uuid128 = u128;

pub const BASE_UUID : Uuid128 = 0x00000000_0000_1000_8000_00805F9B34FBu128;

/// Convert UUID16 into UUID128.
///
/// # Example
///
/// ```rust
/// let u = uuid16_to_uuid128(0xACAB);
/// assert_eq!(u, 0x0000ACAB_0000_1000_8000_00805F9B34FB);
/// ```
pub const fn uuid16_to_uuid128(uuid16: Uuid16) -> Uuid128 {
    return ((uuid16 as u128) << 96) + BASE_UUID;
}

/// Convert UUID32 into UUID128.
///
/// # Example
///
/// ```rust
/// let u = uuid32_to_uuid128(0xFFFFFFFF);
/// assert_eq!(u, 0xFFFFFFFF_0000_1000_8000_00805F9B34FB)
/// ```
pub const fn uuid32_to_uuid128(uuid32: Uuid32) -> Uuid128 {
    return ((uuid32 as u128) << 96) + BASE_UUID;
}

/// Format a UUID128 as a String.
///
/// This follows ITU-T Rec. X.667(10/2012) (aka ISO/IEC 9834-8:2014)
/// Hexadecimal representation (6.4).
///
/// # Example
///
/// ```rust
/// let s = uuid128_to_string(0x0000ACAB_0000_1000_8000_00805F9B34FB);
/// assert_eq!(s, "0000ACAB-0000-1000-8000-00805F9B34FB");
/// ```
///
pub fn uuid128_to_string(uuid128: Uuid128) -> String {
    return format!("{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
                   uuid128 >> 96,
                   uuid128 >> 80 & 0xFFFF,
                   uuid128 >> 64 & 0xFFFF,
                   uuid128 >> 48 & 0xFFFF,
                   uuid128 & 0xFFFFFFFFFFFF);        
}

#[derive(Debug, Error)]
pub enum ParseUuidError {
    #[error("Parsing integers failed")]
    ParseInt(std::num::ParseIntError),
    #[error("Not enough (- separated) parts")]
    Incomplete,
}

impl From<std::num::ParseIntError> for ParseUuidError {
    fn from(err: std::num::ParseIntError) -> ParseUuidError {
        ParseUuidError::ParseInt(err)
    }
}

/// Convert Hexadecimal Repesentation into Uuid128.
///
/// Input is expected to be formatted in ITU-T Rec. X.667(10/2012)
/// (aka ISO/IEC 9834-8:2014) Hexadecimal representation (6.4).
/// ParseUuidError is returned if the String can't be parsed.
///
/// # Example
///
/// ```rust
/// let u = string_to_uuid128("0000ACAB-0000-1000-8000-00805F9B34FB");
/// assert_eq!(u, 0x0000ACAB_0000_1000_8000_00805F9B34FB);
/// ```
///
pub fn string_to_uuid128(s: String) -> Result<Uuid128, ParseUuidError>  {
    let i = s.split('-').map(|x| u128::from_str_radix(x, 16)).collect::<Result<Vec<u128>, std::num::ParseIntError>>()?;
    if i.len() != 5 {
        return Err(ParseUuidError::Incomplete);
    }
    Ok( (i[0] << 96)
         + (i[1] << 80)
         + (i[2] << 64)
         + (i[3] << 48)
         +  i[4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format() {
        let u = 0x0000ACAB_0000_1000_8000_00805F9B34FB;
        let o = string_to_uuid128(uuid128_to_string(u));
        assert!(o.is_ok());
        assert_eq!(o.unwrap(), u);
    }

    #[test]
    fn test_parse_failure() {
        assert!(string_to_uuid128(String::from("0000ACAB-0000-1Z00-8000-00805F9B34FB")).is_err());
        assert!(string_to_uuid128(String::from("0000-1000-8000-00805F9B34FB")).is_err());
    }
}
