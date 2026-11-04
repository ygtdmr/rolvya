//! Strict projection of checksums from Cargo-generated v4 lockfiles, not a general TOML parser.
//! Unsupported formatting/duplicate fields are rejected; Cargo separately validates the full lock.
use crate::files::Result;
use std::collections::BTreeMap;

type Identity = (String, String, String);
pub(crate) fn checksums(text: &str) -> Result<BTreeMap<Identity, String>> {
    let mut sections = text.split("[[package]]");
    let header = sections.next().ok_or("missing Cargo lock header")?;
    if header
        .lines()
        .filter(|s| !s.trim().is_empty() && !s.starts_with('#'))
        .collect::<Vec<_>>()
        != ["version = 4"]
    {
        return Err("unsupported Cargo lock format".into());
    }
    let mut packages = BTreeMap::new();
    for section in sections {
        let mut fields = BTreeMap::new();
        for line in section.lines() {
            for key in ["name", "version", "source", "checksum"] {
                if let Some(raw) = line.strip_prefix(&format!("{key} = ")) {
                    let value: String =
                        serde_json::from_str(raw).map_err(|_| "unsupported Cargo lock string")?;
                    if fields.insert(key, value).is_some() {
                        return Err("duplicate Cargo lock identity field".into());
                    }
                }
            }
        }
        let name = fields.remove("name").ok_or("missing locked package name")?;
        let version = fields
            .remove("version")
            .ok_or("missing locked package version")?;
        let Some(source) = fields.remove("source") else {
            if fields.contains_key("checksum") {
                return Err("unexpected local checksum".into());
            }
            continue;
        };
        if !source.starts_with("registry+") {
            continue;
        }
        let checksum = fields
            .remove("checksum")
            .ok_or("missing locked registry checksum")?;
        if checksum.len() != 64
            || !checksum
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || packages.insert((name, version, source), checksum).is_some()
        {
            return Err("ambiguous registry checksum".into());
        }
    }
    if packages.is_empty() {
        return Err("empty locked registry inventory".into());
    }
    Ok(packages)
}

pub(crate) fn require(expected: &str, bytes: &[u8]) -> Result<String> {
    let actual = crate::files::digest(bytes);
    if actual != expected {
        return Err("cached registry archive differs from Cargo.lock".into());
    }
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_archive_tampering_is_rejected_even_when_source_was_already_extracted() {
        let original = b"reviewed registry archive";
        let expected = crate::files::digest(original);
        assert_eq!(require(&expected, original).unwrap(), expected);
        assert!(
            require(
                &expected,
                b"modified cached archive with existing extracted source"
            )
            .is_err()
        );
    }
    #[test]
    fn generated_lock_projection_rejects_missing_duplicate_or_ambiguous_checksums() {
        let block = format!(
            "\n[[package]]\nname = \"example\"\nversion = \"1.0.0\"\nsource = \"registry+https://example.invalid/index\"\nchecksum = \"{}\"\n",
            "a".repeat(64)
        );
        assert_eq!(
            checksums(&format!("version = 4\n{block}")).unwrap().len(),
            1
        );
        for bad in [
            format!("version = 3\n{block}"),
            format!("version = 4\n{block}{block}"),
            format!("version = 4\n{}", block.replace("checksum =", "other =")),
            format!("version = 4\n{block}\nname = \"substitution\"\n"),
        ] {
            assert!(checksums(&bad).is_err());
        }
    }
}
