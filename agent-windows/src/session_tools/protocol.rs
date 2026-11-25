//! Strict, bounded wire format shared by the clipboard and upload dispatcher.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::de::{self, MapAccess, Visitor};
use serde::Deserializer;
use serde_json::{Map, Value};
use std::fmt;

pub const MAX_FRAME: usize = 16_384;
pub const MAX_TEXT: usize = 4_096;
pub const MAX_CHUNK: usize = 8_192;
pub const MAX_FILE: u64 = 67_108_864;
pub const SESSION_QUOTA: u64 = 268_435_456;

/// Deliberately contains no request data: malformed frames must never be logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidFrame;
impl fmt::Display for InvalidFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid session-tools frame")
    }
}
impl std::error::Error for InvalidFrame {}

#[derive(Debug)]
pub struct Request {
    pub id: String,
    pub op: Operation,
}
#[derive(Debug)]
pub enum Operation {
    ClipboardRead,
    ClipboardWrite(String),
    FileBegin {
        id: String,
        name: String,
        size: u64,
        sha256: String,
    },
    FileChunk {
        id: String,
        offset: u64,
        data: Vec<u8>,
    },
    FileFinish(String),
    FileCancel(String),
}

/// All accepted fields are scalars. Reading the map ourselves also rejects
/// duplicate keys, including a duplicate that would otherwise overwrite a value.
struct StrictObject;
impl<'de> Visitor<'de> for StrictObject {
    type Value = Map<String, Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a unique-key JSON object")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut fields = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if fields.contains_key(&key) {
                return Err(de::Error::custom("duplicate field"));
            }
            let value = map.next_value::<Value>()?;
            if value.is_array() || value.is_object() || fields.len() >= 8 {
                return Err(de::Error::custom("invalid field shape"));
            }
            fields.insert(key, value);
        }
        Ok(fields)
    }
}

/// Shared with the control channel: both protocols accept only a bounded set of
/// scalar fields, and neither may silently overwrite a duplicated JSON key.
pub(crate) fn scalar_object(bytes: &[u8]) -> Result<Map<String, Value>, InvalidFrame> {
    if bytes.len() > MAX_FRAME {
        return Err(InvalidFrame);
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let object = de.deserialize_map(StrictObject).map_err(|_| InvalidFrame)?;
    de.end().map_err(|_| InvalidFrame)?;
    Ok(object)
}

pub fn parse(bytes: &[u8]) -> Result<Request, InvalidFrame> {
    let object = scalar_object(bytes)?;
    if number(&object, "v")? != 1 {
        return Err(InvalidFrame);
    }
    let kind = string(&object, "type")?;
    let id = string(&object, "requestId")?;
    if !lower_hex(id, 32) {
        return Err(InvalidFrame);
    }
    let allowed: &[&str] = match kind {
        "clipboard_read" => &["v", "type", "requestId"],
        "clipboard_write" => &["v", "type", "requestId", "text"],
        "file_begin" => &[
            "v",
            "type",
            "requestId",
            "transferId",
            "name",
            "size",
            "sha256",
        ],
        "file_chunk" => &["v", "type", "requestId", "transferId", "offset", "data"],
        "file_finish" | "file_cancel" => &["v", "type", "requestId", "transferId"],
        _ => return Err(InvalidFrame),
    };
    if object.len() != allowed.len() || !object.keys().all(|key| allowed.contains(&key.as_str())) {
        return Err(InvalidFrame);
    }
    let transfer_id = || -> Result<String, InvalidFrame> {
        let transfer = string(&object, "transferId")?;
        if !lower_hex(transfer, 32) || transfer == id {
            return Err(InvalidFrame);
        }
        Ok(transfer.into())
    };
    let op = match kind {
        "clipboard_read" => Operation::ClipboardRead,
        "clipboard_write" => {
            let text = string(&object, "text")?;
            if !valid_text(text) {
                return Err(InvalidFrame);
            }
            Operation::ClipboardWrite(text.into())
        }
        "file_begin" => {
            let name = string(&object, "name")?;
            let size = number(&object, "size")?;
            let sha256 = string(&object, "sha256")?;
            if !valid_name(name) || size > MAX_FILE || !lower_hex(sha256, 64) {
                return Err(InvalidFrame);
            }
            Operation::FileBegin {
                id: transfer_id()?,
                name: name.into(),
                size,
                sha256: sha256.into(),
            }
        }
        "file_chunk" => {
            let offset = number(&object, "offset")?;
            let encoded = string(&object, "data")?;
            if offset > MAX_FILE || encoded.len() > MAX_CHUNK.div_ceil(3) * 4 {
                return Err(InvalidFrame);
            }
            let data = STANDARD.decode(encoded).map_err(|_| InvalidFrame)?;
            // Re-encoding enforces canonical alphabet, padding and zero pad bits.
            if data.len() > MAX_CHUNK || STANDARD.encode(&data) != encoded {
                return Err(InvalidFrame);
            }
            Operation::FileChunk {
                id: transfer_id()?,
                offset,
                data,
            }
        }
        "file_finish" => Operation::FileFinish(transfer_id()?),
        "file_cancel" => Operation::FileCancel(transfer_id()?),
        _ => return Err(InvalidFrame),
    };
    Ok(Request { id: id.into(), op })
}

fn string<'a>(object: &'a Map<String, Value>, name: &str) -> Result<&'a str, InvalidFrame> {
    object.get(name).and_then(Value::as_str).ok_or(InvalidFrame)
}
fn number(object: &Map<String, Value>, name: &str) -> Result<u64, InvalidFrame> {
    object.get(name).and_then(Value::as_u64).ok_or(InvalidFrame)
}
pub fn lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn valid_text(value: &str) -> bool {
    value.len() <= MAX_TEXT && !value.contains('\0')
}
pub fn valid_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 120
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return false;
    }
    // Windows recognizes these device names even with an extension. It also
    // recognizes the superscript 1, 2 and 3 variants of COM/LPT device numbers.
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return false;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(suffix) = stem.strip_prefix(prefix) {
            if matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            ) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const ID: &str = "0123456789abcdef0123456789abcdef";
    const TRANSFER: &str = "abcdef0123456789abcdef0123456789";
    fn read() -> Value {
        json!({"v":1,"type":"clipboard_read","requestId":ID})
    }
    fn begin() -> Value {
        json!({"v":1,"type":"file_begin","requestId":ID,"transferId":TRANSFER,"name":"x.txt","size":0,"sha256":"a".repeat(64)})
    }
    #[test]
    fn rejects_duplicate_fields_before_overwrite() {
        for fields in [
            format!(r#""v":2,"v":1,"type":"clipboard_read","requestId":"{ID}""#),
            format!(
                r#""v":1,"type":"clipboard_write","requestId":"{ID}","text":"secret","text":"""#
            ),
            format!(r#""v":1,"type":"clipboard_read","requestId":"wrong","requestId":"{ID}""#),
        ] {
            assert!(parse(format!("{{{fields}}}").as_bytes()).is_err());
        }
    }
    #[test]
    fn rejects_unknown_versions_fields_types_and_trailing_json() {
        for (key, value) in [
            ("v", json!(1.0)),
            ("v", json!(2)),
            ("requestId", json!(ID.to_uppercase())),
            ("extra", json!(true)),
            ("type", json!("execute")),
            ("requestId", json!({"a":1,"b":2})),
        ] {
            let mut r = read();
            r[key] = value;
            assert!(parse(r.to_string().as_bytes()).is_err());
        }
        assert!(parse(format!("{} {{}}", read()).as_bytes()).is_err());
        assert!(parse(&vec![b' '; MAX_FRAME + 1]).is_err());
        assert!(parse(&[0xff]).is_err());
        assert!(parse(b"[]").is_err());
    }
    #[test]
    fn names_cannot_select_paths_or_windows_devices() {
        for name in [
            "",
            ".",
            "..",
            "../a",
            "a/b",
            "a\\b",
            "C:boot.ini",
            "a:stream",
            "CON",
            "nul.txt",
            "Lpt9.doc",
            "COM¹.txt",
            "CON .log",
            "CONOUT$",
            "x.",
            "x ",
            "x\n",
            "a?b",
        ] {
            assert!(!valid_name(name), "{name:?}");
        }
        assert!(valid_name("résumé 2026.pdf"));
        assert!(valid_name("COM10.txt"));
        assert!(!valid_name(&"é".repeat(61)));
    }
    #[test]
    fn text_limits_are_utf8_bytes_and_allow_empty() {
        for text in ["".to_owned(), "é".repeat(MAX_TEXT / 2)] {
            let v = json!({"v":1,"type":"clipboard_write","requestId":ID,"text":text});
            assert!(parse(v.to_string().as_bytes()).is_ok());
        }
        for text in ["é".repeat(MAX_TEXT / 2 + 1), "a\0b".into()] {
            let v = json!({"v":1,"type":"clipboard_write","requestId":ID,"text":text});
            assert!(parse(v.to_string().as_bytes()).is_err());
        }
    }
    #[test]
    fn file_bounds_and_integers_fail_closed() {
        assert!(parse(begin().to_string().as_bytes()).is_ok());
        for (key, value) in [
            ("size", json!(MAX_FILE + 1)),
            ("size", json!(-1)),
            ("size", json!(1.0)),
            ("sha256", json!("A".repeat(64))),
            ("transferId", json!(ID)),
        ] {
            let mut r = begin();
            r[key] = value;
            assert!(parse(r.to_string().as_bytes()).is_err());
        }
    }
    #[test]
    fn chunks_are_bounded_canonical_base64() {
        let chunk = |data: &str| json!({"v":1,"type":"file_chunk","requestId":ID,"transferId":TRANSFER,"offset":0,"data":data});
        for data in ["YQ", "YR==", "YQ==\n", "____", "="] {
            assert!(parse(chunk(data).to_string().as_bytes()).is_err());
        }
        assert!(parse(
            chunk(&STANDARD.encode(vec![1; MAX_CHUNK]))
                .to_string()
                .as_bytes()
        )
        .is_ok());
        assert!(parse(chunk("").to_string().as_bytes()).is_ok());
        assert!(parse(
            chunk(&STANDARD.encode(vec![1; MAX_CHUNK + 1]))
                .to_string()
                .as_bytes()
        )
        .is_err());
    }
}
