//! What the app may hand to the system browser or Telegram: only our own pages and the bot.
pub fn allowed_external(url: &str) -> bool {
    let safe = url.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'\'' | b'\\' | b'`' | b'<' | b'>' | b'^' | b'|' | b'{' | b'}'));
    safe && (url.starts_with("https://marzvpn.com/")
        || url == "https://t.me/MarzReality_bot"
        || url.starts_with("https://t.me/MarzReality_bot?start=")
        || url == "https://www.gnu.org/licenses/gpl-3.0.html"
        || url == "https://github.com/amberfield2931/MARZ-VPN-Apps"
        || url == "mailto:support@marzvpn.com?subject=MARZ%20VPN%20App")
}

/// The Telegram code sign-in page as the App API forms it: `https://marzvpn.com[/ru|/fa|/zh-cn]/device/#c=<claim>`.
/// Only this exact shape is opened from a start answer; anything else stays closed (the allow-list is not widened).
pub fn is_device_page(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://marzvpn.com") else { return false };
    let rest = ["/ru", "/fa", "/zh-cn"].iter().find_map(|p| rest.strip_prefix(p).filter(|r| r.starts_with("/device/"))).unwrap_or(rest);
    let Some(claim) = rest.strip_prefix("/device/#c=") else { return false };
    allowed_external(url) && (32..=64).contains(&claim.len()) && claim.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::{allowed_external, is_device_page};

    #[test]
    fn only_the_exact_sign_in_page_opens_from_a_start_answer() {
        let claim = "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-AbCd";
        for lang in ["", "/ru", "/fa", "/zh-cn"] {
            assert!(is_device_page(&format!("https://marzvpn.com{lang}/device/#c={claim}")), "{lang}");
        }
        for bad in [
            format!("https://marzvpn.com.evil.com/device/#c={claim}"),
            format!("http://marzvpn.com/device/#c={claim}"),
            format!("https://evil.com/device/#c={claim}"),
            format!("https://marzvpn.com/de/device/#c={claim}"),
            format!("https://marzvpn.com/device/?x=1#c={claim}"),
            format!("https://marzvpn.com/device/#c={claim}\"&calc"),
            "https://marzvpn.com/device/#c=short".to_string(),
            format!("https://marzvpn.com/account/#c={claim}"),
            format!("https://marzvpn.com/ru/device/#c={claim}/../x"),
            format!("https://marzvpn.com@evil.com/device/#c={claim}"),
        ] {
            assert!(!is_device_page(&bad), "{bad}");
        }
    }

    #[test]
    fn the_source_repository_opens_and_nothing_else_on_github() {
        assert!(allowed_external("https://github.com/amberfield2931/MARZ-VPN-Apps"));
        assert!(!allowed_external("https://github.com/amberfield2931/MARZ-VPN-Apps/evil"));
        assert!(!allowed_external("https://github.com/someone/else"));
    }

    #[test]
    fn only_our_destinations_open() {
        for ok in ["https://marzvpn.com/ru/terms", "https://t.me/MarzReality_bot", "https://t.me/MarzReality_bot?start=applogin_abc", "https://www.gnu.org/licenses/gpl-3.0.html", "mailto:support@marzvpn.com?subject=MARZ%20VPN%20App"] {
            assert!(allowed_external(ok), "{ok}");
        }
        for bad in ["http://marzvpn.com/", "https://marzvpn.com.evil.com/", "https://evil.com/", "file:///etc/passwd", "https://t.me/OtherBot", "https://marzvpn.com/ x", "--help", "javascript:alert(1)", "https://marzvpn.com/\"&calc"] {
            assert!(!allowed_external(bad), "{bad}");
        }
    }
}
