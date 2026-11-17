//! Checks marzvpn.com for a newer published version. It only reads one small public file and tells the user: nothing is
//! downloaded or installed, no token, account or device data is sent, and the helper, the VPN and the Kill Switch are not involved.
use crate::http::Endpoint;
use serde_json::Value;

pub const SITE: &str = "https://marzvpn.com";
pub const MANIFEST_PATH: &str = "/updates/manifest.json";
pub const MAX_MANIFEST: usize = 64 * 1024;
/// The automatic check at start runs at most this often. The user's button is not limited.
pub const EVERY_SECS: i64 = 24 * 60 * 60;

/// major.minor.patch with an optional package revision: 0.1.7, 0.1.2-6. Anything else (rc, a test suffix) is not a published version.
fn parts(version: &str) -> Option<[u32; 4]> {
    let (main, revision) = version.split_once('-').map_or((version, None), |(m, r)| (m, Some(r)));
    let number = |s: &str| (!s.is_empty() && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse::<u32>().ok()).flatten();
    let mut it = main.split('.');
    let out = [number(it.next()?)?, number(it.next()?)?, number(it.next()?)?, revision.map_or(Some(0), number)?];
    it.next().is_none().then_some(out)
}

/// None when either side is not a published version: the caller reports a failed check, never a false "new version".
pub fn is_newer(latest: &str, current: &str) -> Option<bool> {
    Some(parts(latest)? > parts(current)?)
}

/// The published version for this platform (linux | windows). Each platform has its own entry and its own numbering.
pub fn latest_for(manifest: &[u8], platform: &str) -> Option<String> {
    if manifest.len() > MAX_MANIFEST {
        return None;
    }
    let j: Value = serde_json::from_slice(manifest).ok()?;
    if j.get("schema")?.as_u64()? != 1 {
        return None;
    }
    let entry = j.get("platforms")?.get(platform)?;
    if !matches!(entry.get("channel")?.as_str()?, "stable" | "beta") {
        return None;
    }
    let version = entry.get("version")?.as_str()?;
    parts(version).map(|_| version.to_string())
}

/// A clock moved back must not stop the checks for good.
pub fn due(last_check: i64, now: i64) -> bool {
    last_check <= 0 || last_check > now || now - last_check >= EVERY_SECS
}

/// Everything the check sends: no cookie, no token, no version, no device or account header.
pub const REQUEST_HEADERS: [(&str, &str); 2] = [("Accept", "application/json"), ("User-Agent", "MARZ-VPN")];

pub fn fetch(endpoint: &Endpoint) -> Result<Vec<u8>, ()> {
    let headers = REQUEST_HEADERS.map(|(k, v)| (k, v.to_string()));
    let reply = endpoint.request("GET", MANIFEST_PATH, &headers, &[], None).map_err(|_| ())?;
    (reply.code == 200 && reply.body.len() <= MAX_MANIFEST).then_some(reply.body).ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        for (latest, current, newer) in [
            ("0.1.2-7", "0.1.2-6", true),
            ("0.1.2-6", "0.1.2-6", false),
            ("0.1.2-5", "0.1.2-6", false),
            ("0.1.2-10", "0.1.2-9", true),
            ("0.1.2-6", "0.1.2", true),
            ("0.1.2", "0.1.2-6", false),
            ("0.1.3", "0.1.2-6", true),
            ("0.2.0", "0.1.2-6", true),
            ("0.1.10", "0.1.7", true),
            ("0.1.7", "0.1.10", false),
            ("0.1.8", "0.1.7", true),
            ("0.1.7", "0.1.7", false),
            ("1.0.1", "1.0.1", false),
            ("1.0.10", "1.0.9", true),
            ("10.0.0", "9.9.9", true),
        ] {
            assert_eq!(is_newer(latest, current), Some(newer), "{latest} against {current}");
        }
    }

    #[test]
    fn anything_but_a_published_version_is_refused() {
        for bad in ["", "1", "1.0", "1.0.1.2", "v1.0.1", "1.0.0-rc6", "0.1.2-", "0.1.2-6-1", "0.1.x", "-1.0.0", "1.0.1 ", "1..1", "99999999999.0.0", "lab", "١.٠.١"] {
            assert_eq!(is_newer(bad, "0.1.7"), None, "{bad}");
            assert_eq!(is_newer("0.1.7", bad), None, "{bad}");
        }
    }

    const MANIFEST: &str = r#"{"schema":1,"platforms":{"android":{"channel":"stable","version":"1.0.1","version_code":1000092},"linux":{"channel":"stable","version":"0.1.2-6"},"windows":{"channel":"beta","version":"0.1.7"}}}"#;

    #[test]
    fn each_platform_reads_its_own_entry() {
        assert_eq!(latest_for(MANIFEST.as_bytes(), "linux").as_deref(), Some("0.1.2-6"));
        assert_eq!(latest_for(MANIFEST.as_bytes(), "windows").as_deref(), Some("0.1.7"));
        assert_eq!(latest_for(MANIFEST.as_bytes(), "macos"), None);
    }

    #[test]
    fn a_bad_manifest_is_a_failed_check() {
        for bad in [
            "",
            "<html>",
            "[]",
            r#"{"schema":2,"platforms":{"linux":{"channel":"stable","version":"9.9.9"}}}"#,
            r#"{"platforms":{"linux":{"channel":"stable","version":"9.9.9"}}}"#,
            r#"{"schema":1,"platforms":{"linux":{"channel":"test","version":"9.9.9"}}}"#,
            r#"{"schema":1,"platforms":{"linux":{"version":"9.9.9"}}}"#,
            r#"{"schema":1,"platforms":{"linux":{"channel":"stable","version":"9.9.9-rc1"}}}"#,
            r#"{"schema":1,"platforms":{"linux":{"channel":"stable","version":9}}}"#,
        ] {
            assert_eq!(latest_for(bad.as_bytes(), "linux"), None, "{bad}");
        }
        let mut huge = MANIFEST.as_bytes().to_vec();
        huge.resize(MAX_MANIFEST + 1, b' ');
        assert_eq!(latest_for(&huge, "linux"), None);
    }

    #[test]
    fn at_most_once_in_24_hours() {
        let now = 1_800_000_000;
        assert!(due(0, now));
        assert!(!due(now, now));
        assert!(!due(now - EVERY_SECS + 1, now));
        assert!(due(now - EVERY_SECS, now));
        assert!(due(now + 3600, now), "a clock moved back");
    }

    /// Manual, needs the internet: the real client and its fixed User-Agent get through Cloudflare to the site.
    /// `cargo test -p marz-core live_site -- --ignored`
    #[test]
    #[ignore]
    fn live_site_answers_the_client() {
        let endpoint = Endpoint::parse(SITE).unwrap();
        let headers = REQUEST_HEADERS.map(|(k, v)| (k, v.to_string()));
        let r = endpoint.request("GET", "/downloads/notices/SHA256SUMS.txt", &headers, &[], None).unwrap();
        assert_eq!(r.code, 200);
        assert!(String::from_utf8_lossy(&r.body).contains("linux/MARZ-VPN-"));
        match fetch(&endpoint) {
            Ok(body) => assert!(latest_for(&body, "linux").is_some(), "a published manifest must parse"),
            Err(()) => eprintln!("updates/manifest.json is not on the site yet: the apps report a failed check"),
        }
    }

    #[test]
    fn the_request_carries_nothing_personal() {
        for (name, value) in REQUEST_HEADERS {
            let line = format!("{name} {value}").to_ascii_lowercase();
            for word in ["authorization", "cookie", "token", "device", "version", "account"] {
                assert!(!line.contains(word), "{line}");
            }
        }
    }
}
