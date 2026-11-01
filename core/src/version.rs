pub fn newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| -> Vec<u64> { version.split('.').map(|part| part.parse().unwrap_or(0)).chain([0; 3]).take(3).collect() };
    parse(candidate) > parse(current)
}

/// The string value of a top-level `key` in GitHub's JSON, without a JSON parser for one field.
pub fn field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let rest = &json[json.find(&format!("\"{key}\""))? + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(&rest[..rest.find('"')?])
}

/// The string value of `key` inside the release asset called `name`, as its download address or digest.
/// GitHub lists each asset's fields after its name, so the first `key` past the name belongs to it.
pub fn asset_field<'a>(json: &'a str, name: &str, key: &str) -> Option<&'a str> {
    let start = json.find(&format!("\"name\":\"{name}\"")).or_else(|| json.find(&format!("\"name\": \"{name}\"")))?;
    let asset = &json[start..];
    // Stop at the next asset, so a missing field isn't taken from it.
    let end = asset[1..].find("\"name\":").map_or(asset.len(), |i| i + 1);
    field(&asset[..end], key)
}

/// The SHA-256 a checksum file such as `SHA256SUMS.txt` lists for `name`, in lowercase hex.
pub fn listed_digest(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())).then(|| digest.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.4.0", "0.3.1"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.4.0", "0.4.0"));
        assert!(!newer("0.3.9", "0.4.0"));
    }

    #[test]
    fn reads_a_field() {
        assert_eq!(field(r#"{"url":"x","tag_name": "v0.4.0","name":"Snowhush 0.4.0"}"#, "tag_name"), Some("v0.4.0"));
        assert_eq!(field(r#"{"name":"x"}"#, "tag_name"), None);
    }

    #[test]
    fn reads_an_asset_field() {
        let json = r#"{"tag_name":"v1","assets":[{"name":"SHA256SUMS.txt","digest":"sha256:aa","browser_download_url":"https://x/sums"},
            {"name":"Snowhush-Setup.exe","label":null,"uploader":{"login":"me"},"digest":"sha256:bb","browser_download_url":"https://x/setup"},
            {"name":"other","browser_download_url":"https://x/other"}]}"#;
        assert_eq!(asset_field(json, "Snowhush-Setup.exe", "digest"), Some("sha256:bb"));
        assert_eq!(asset_field(json, "Snowhush-Setup.exe", "browser_download_url"), Some("https://x/setup"));
        assert_eq!(asset_field(json, "other", "digest"), None);
        assert_eq!(asset_field(json, "missing", "digest"), None);
    }

    #[test]
    fn reads_a_checksum_file() {
        let sums = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF *Snowhush-Setup.exe\nnot a line\n";
        assert_eq!(listed_digest(sums, "Snowhush-Setup.exe").as_deref(), Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"));
        assert_eq!(listed_digest(sums, "Other.exe"), None);
    }
}
