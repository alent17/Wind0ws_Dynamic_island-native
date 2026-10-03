//! Stable player identity shared by the media service, UI and extensions.

/// A media provider identified from its GSMTC source application identity.
///
/// Classification is intentionally based only on the session identity. Track
/// metadata and window titles must never opt a generic player into an adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlayerKind {
    NetEaseCloudMusic,
    Spotify,
    QqMusic,
    MicrosoftEdge,
    Chrome,
    Firefox,
    AppleMusic,
    #[default]
    Other,
}

impl PlayerKind {
    /// Classifies a GSMTC `SourceAppUserModelId` or equivalent app identity.
    /// Unknown and empty values safely remain on the generic GSMTC path.
    pub fn from_session_identity(identity: &str) -> Self {
        let identity = identity.to_ascii_lowercase();
        if identity
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| matches!(part, "cloudmusic" | "neteasecloudmusic" | "neteasemusic"))
        {
            Self::NetEaseCloudMusic
        } else if identity.contains("spotify") {
            Self::Spotify
        } else if identity.contains("qqmusic") {
            Self::QqMusic
        } else if identity.contains("msedge") {
            Self::MicrosoftEdge
        } else if identity.contains("chrome") {
            Self::Chrome
        } else if identity.contains("firefox") {
            Self::Firefox
        } else if identity.contains("applemusic") {
            Self::AppleMusic
        } else {
            Self::Other
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::NetEaseCloudMusic => "网易云音乐",
            Self::Spotify => "Spotify",
            Self::QqMusic => "QQ 音乐",
            Self::MicrosoftEdge => "Microsoft Edge",
            Self::Chrome => "Chrome",
            Self::Firefox => "Firefox",
            Self::AppleMusic => "Apple Music",
            Self::Other => "媒体播放器",
        }
    }

    pub const fn uses_netease_extension(self) -> bool {
        matches!(self, Self::NetEaseCloudMusic)
    }
}

#[cfg(test)]
mod tests {
    use super::PlayerKind;

    #[test]
    fn classifies_known_session_identities_without_case_sensitivity() {
        let cases = [
            ("cloudmusic.exe", PlayerKind::NetEaseCloudMusic),
            ("com.netease.cloudmusic", PlayerKind::NetEaseCloudMusic),
            ("NeteaseMusic.exe", PlayerKind::NetEaseCloudMusic),
            ("Spotify.exe", PlayerKind::Spotify),
            ("QQMusic", PlayerKind::QqMusic),
            ("MSEdge", PlayerKind::MicrosoftEdge),
            ("Chrome", PlayerKind::Chrome),
            ("Firefox", PlayerKind::Firefox),
            ("AppleMusic", PlayerKind::AppleMusic),
        ];
        for (identity, expected) in cases {
            assert_eq!(PlayerKind::from_session_identity(identity), expected);
        }
    }

    #[test]
    fn unknown_or_empty_identity_stays_on_the_generic_path() {
        assert_eq!(PlayerKind::from_session_identity(""), PlayerKind::Other);
        assert_eq!(
            PlayerKind::from_session_identity("LocalPlayer.exe"),
            PlayerKind::Other
        );
        assert_eq!(
            PlayerKind::from_session_identity("com.netease.game"),
            PlayerKind::Other
        );
        assert_eq!(
            PlayerKind::from_session_identity("netease"),
            PlayerKind::Other
        );
        assert_eq!(
            PlayerKind::from_session_identity("MyCloudMusicFanApp.exe"),
            PlayerKind::Other
        );
        assert_eq!(
            PlayerKind::from_session_identity("Cloud Music Fan Page"),
            PlayerKind::Other
        );
        assert_eq!(PlayerKind::Other.display_name(), "媒体播放器");
        assert!(!PlayerKind::Other.uses_netease_extension());
        assert!(PlayerKind::NetEaseCloudMusic.uses_netease_extension());
    }
}
