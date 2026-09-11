//! Where the shared header chrome lives at any given moment.
//!
//! Two widget groups have no fixed home, because the header bar that would
//! own them can be hidden by a sidebar toggle or a breakpoint:
//!
//! * the left pair (open-workspace button + hamburger menu) — belongs to the
//!   repo sidebar's header while that sidebar is on screen, otherwise to the
//!   content header;
//! * the window controls (minimise/maximise/close) — owned by the right
//!   sidebar's header only while that sidebar is laid out beside the content.
//!   Hidden or overlaying, the content header takes them, so the close button
//!   never disappears with the panel.
//!
//! The decision is pure so it can be unit-tested without a display; `window.rs`
//! feeds it split-view state and applies the result.

/// Placement of the movable header chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromePlacement {
    /// Open-workspace button + hamburger sit in the repo sidebar's header.
    pub left_pair_in_sidebar: bool,
    /// Content header shows the window controls (the right sidebar's header
    /// shows them regardless — it is the topmost bar when it is visible).
    pub window_controls_in_content: bool,
}

/// Decide placement from the two split views' state.
///
/// `left_shows` ignores `collapsed` on purpose: an overlaying repo sidebar
/// covers the content header on phone widths, so the open-workspace button has
/// to travel with the sidebar to stay reachable.
pub fn placement(left_shows: bool, right_shows: bool, right_collapsed: bool) -> ChromePlacement {
    let right_is_inline = right_shows && !right_collapsed;
    ChromePlacement {
        left_pair_in_sidebar: left_shows,
        window_controls_in_content: !right_is_inline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_pair_follows_the_repo_sidebar() {
        assert!(placement(true, true, false).left_pair_in_sidebar);
        assert!(!placement(false, true, false).left_pair_in_sidebar);
    }

    #[test]
    fn left_pair_travels_with_an_overlaying_sidebar() {
        // Collapsed + shown = overlay covering the content header.
        assert!(placement(true, false, true).left_pair_in_sidebar);
    }

    #[test]
    fn inline_right_sidebar_owns_the_window_controls() {
        assert!(!placement(true, true, false).window_controls_in_content);
    }

    #[test]
    fn hiding_the_right_sidebar_hands_controls_back() {
        // The regression this guards: toggling the panel off on a wide window
        // used to take the close button with it.
        assert!(placement(true, false, false).window_controls_in_content);
    }

    #[test]
    fn overlaying_right_sidebar_leaves_controls_in_content() {
        assert!(placement(true, true, true).window_controls_in_content);
        assert!(placement(true, false, true).window_controls_in_content);
    }
}
