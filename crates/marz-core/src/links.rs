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

#[cfg(test)]
mod tests {
    use super::allowed_external;

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
