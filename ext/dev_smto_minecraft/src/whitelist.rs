//! `whitelist.json` handling, including the offline-mode UUID scheme.
//!
//! Minecraft stores the whitelist as a JSON array of `{ "uuid", "name" }`. On
//! online-mode servers the UUID is the Mojang account UUID. On offline-mode
//! servers it is a version 3 (MD5) UUID derived from `"OfflinePlayer:<name>"`,
//! which is what `UUID.nameUUIDFromBytes` produces in Java.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub uuid: String,
    pub name: String,
}

pub fn parse(input: &str) -> Result<Vec<Entry>, serde_json::Error> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<Entry> = serde_json::from_str(input)?;

    // Minecraft rewrites the file with lowercased UUIDs, keep it stable.
    for entry in &mut entries {
        entry.uuid.make_ascii_lowercase();
    }

    Ok(entries)
}

pub fn render(entries: &[Entry]) -> String {
    let mut json = serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string());
    json.push('\n');
    json
}

/// The UUID an offline-mode server derives from a player name.
pub fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};

    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();

    // UUID version 3, IETF variant.
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Accepts the names Mojang accepts: 1-16 of `A-Z a-z 0-9 _`.
pub fn is_valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns an error message when the name cannot be whitelisted.
pub fn validate_name(name: &str) -> Result<(), String> {
    if is_valid_name(name) {
        Ok(())
    } else {
        Err(format!(
            "'{name}' is not a valid Minecraft username (1-16 letters, digits or underscores)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuids_match_java() {
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("jeb_"), "a762f560-4fce-3236-812a-b80efff0b62b");
        assert_eq!(
            offline_uuid("Dinnerbone"),
            "4d258a81-2358-3084-8166-05b9faccad80"
        );
    }

    #[test]
    fn round_trips_entries() {
        let entries = vec![
            Entry {
                uuid: "b50ad385-829d-3141-a216-7e7d7539ba7f".into(),
                name: "Notch".into(),
            },
            Entry {
                uuid: "a762f560-4fce-3236-812a-b80efff0b62b".into(),
                name: "jeb_".into(),
            },
        ];

        let rendered = render(&entries);
        assert_eq!(parse(&rendered).unwrap(), entries);
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn treats_empty_file_as_empty_list() {
        assert_eq!(parse("").unwrap(), Vec::new());
        assert_eq!(parse("  \n").unwrap(), Vec::new());
    }

    #[test]
    fn rejects_broken_json() {
        assert!(parse("[{\"uuid\":\"x\"}]").is_err());
    }

    #[test]
    fn validates_names() {
        assert!(is_valid_name("Notch"));
        assert!(is_valid_name("a_b_1"));
        assert!(is_valid_name("aaaaaaaaaaaaaaaa"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("aaaaaaaaaaaaaaaaa"));
        assert!(!is_valid_name("has space"));
        assert!(!is_valid_name("dash-name"));
    }
}
