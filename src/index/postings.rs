use anyhow::Result;

pub fn encode_varint(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

pub fn decode_varint(data: &[u8]) -> Result<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0;
    for (i, &byte) in data.iter().enumerate() {
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
        shift += 7;
        if shift >= 35 {
            anyhow::bail!("varint too long");
        }
    }
    anyhow::bail!("unexpected end of varint");
}

pub fn encode_posting_list(ids: &[u32]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut prev = 0u32;
    for &id in ids {
        encode_varint(id - prev, &mut buf);
        prev = id;
    }
    buf
}

pub fn decode_posting_list(data: &[u8]) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut offset = 0;
    let mut prev = 0u32;
    while offset < data.len() {
        if let Ok((delta, consumed)) = decode_varint(&data[offset..]) {
            prev += delta;
            ids.push(prev);
            offset += consumed;
        } else {
            break;
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        for &v in &[0u32, 1, 127, 128, 255, 256, 16383, 16384, u32::MAX] {
            let mut buf = Vec::new();
            encode_varint(v, &mut buf);
            let (decoded, bytes_read) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, v);
            assert_eq!(bytes_read, buf.len());
        }
    }

    #[test]
    fn test_posting_list_roundtrip() {
        let ids = vec![5, 10, 15, 100, 1000, 50000];
        let encoded = encode_posting_list(&ids);
        let decoded = decode_posting_list(&encoded);
        assert_eq!(decoded, ids);
    }

    #[test]
    fn test_posting_list_empty() {
        let encoded = encode_posting_list(&[]);
        assert!(decode_posting_list(&encoded).is_empty());
    }

    #[test]
    fn test_posting_list_single() {
        let encoded = encode_posting_list(&[42]);
        assert_eq!(decode_posting_list(&encoded), vec![42]);
    }
}
