//! Who may change a VPN session. The identity comes from the operating system (SO_PEERCRED on Linux, the pipe
//! client's token on Windows), never from anything in a request.
//!
//! The VPN switch and its firewall block belong to the user who turned them on. Another ordinary user of the
//! computer can see that a session exists, and nothing else, and can change nothing. An administrator always
//! can, so a computer can never be left without a network for good.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// Stable identity of the user: "uid:1000" or a Windows SID.
    pub id: String,
    /// root, or SYSTEM / an elevated administrator.
    pub admin: bool,
}

pub const NOT_OWNER: &str = "not_session_owner";

/// May this peer start, change or end the session? With no session, anyone allowed to connect may start one.
/// A session with no recorded owner (a damaged saved state) belongs to administrators: fail closed.
pub fn may_control(peer: &Peer, session_active: bool, owner: Option<&str>) -> bool {
    peer.admin || !session_active || owner == Some(peer.id.as_str())
}

/// May this peer see the routes in use and the session's errors?
pub fn may_see(peer: &Peer, owner: Option<&str>) -> bool {
    peer.admin || owner == Some(peer.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: &str) -> Peer {
        Peer { id: id.into(), admin: false }
    }

    #[test]
    fn another_user_cannot_touch_a_running_session() {
        assert!(!may_control(&user("uid:1002"), true, Some("uid:1001")));
        assert!(!may_see(&user("uid:1002"), Some("uid:1001")));
    }

    #[test]
    fn the_owner_can() {
        assert!(may_control(&user("uid:1001"), true, Some("uid:1001")));
        assert!(may_see(&user("uid:1001"), Some("uid:1001")));
    }

    #[test]
    fn an_administrator_always_can() {
        let root = Peer { id: "uid:0".into(), admin: true };
        assert!(may_control(&root, true, Some("uid:1001")));
        assert!(may_control(&root, true, None));
        assert!(may_see(&root, Some("uid:1001")));
    }

    #[test]
    fn with_no_session_anyone_may_start_one() {
        assert!(may_control(&user("uid:1002"), false, None));
    }

    #[test]
    fn a_session_without_a_recorded_owner_is_for_administrators_only() {
        assert!(!may_control(&user("uid:1001"), true, None));
        assert!(!may_see(&user("uid:1001"), None));
    }

    #[test]
    fn ids_must_match_exactly() {
        assert!(!may_control(&user("uid:100"), true, Some("uid:1001")));
        assert!(!may_control(&user(""), true, Some("uid:1001")));
    }
}
