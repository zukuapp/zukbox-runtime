//! CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320).
//!
//! 파일 헤더 무결성과 `HAS_CRC` 청크 검증에 쓰인다.

/// 컴파일 타임에 만들어지는 256엔트리 룩업 테이블 (1 KiB).
const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// `bytes` 의 CRC-32 를 계산한다.
pub fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ TABLE[index];
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::checksum;

    #[test]
    fn known_vectors() {
        // RFC 3720 / zlib 표준 검증 벡터.
        assert_eq!(checksum(b""), 0x0000_0000);
        assert_eq!(checksum(b"a"), 0xE8B7_BE43);
        assert_eq!(checksum(b"123456789"), 0xCBF4_3926);
    }
}
