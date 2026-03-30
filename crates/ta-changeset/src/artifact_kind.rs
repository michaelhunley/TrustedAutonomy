// artifact_kind.rs — ArtifactKind enum for typed artifact metadata (v0.14.15).
//
// Describes the semantic kind of an artifact so the draft review pipeline can
// render appropriate summaries. For example, binary image artifacts suppress
// the text diff and show a human-readable frame/resolution summary instead.

use serde::{Deserialize, Serialize};

/// Semantic kind of an artifact produced by a connector or agent.
///
/// Stored on [`Artifact`] as an optional field. When absent, the artifact is
/// treated as a generic file. The `ta draft view` renderer uses this to pick
/// the appropriate display format (e.g. suppress binary diffs for images).
///
/// # Extensibility
/// Future connectors can add new variants here (e.g. `Audio`, `Video`,
/// `PointCloud`). The `Image` variant is intentionally generic — it is not
/// tied to Unreal Engine or any other specific connector.
///
/// [`Artifact`]: crate::draft_package::Artifact
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A raster image artifact (PNG, EXR, JPEG, …).
    ///
    /// Fields are optional — connectors fill in what they know. Width and
    /// height are in pixels; `frame_index` is zero-based within a sequence.
    Image {
        /// Image width in pixels, if known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        /// Image height in pixels, if known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
        /// File format string, e.g. `"PNG"`, `"EXR"`, `"JPEG"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        /// Zero-based frame index within a render sequence, if applicable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frame_index: Option<u32>,
    },
}

impl ArtifactKind {
    /// Returns true if this is an image kind (binary; diff should be suppressed).
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    /// Returns a short human-readable label for display (e.g. `"PNG image"`).
    pub fn display_label(&self) -> String {
        match self {
            Self::Image { format, .. } => match format.as_deref() {
                Some(fmt) => format!("{} image", fmt),
                None => "image".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_roundtrip_full() {
        let kind = ArtifactKind::Image {
            width: Some(1024),
            height: Some(1024),
            format: Some("PNG".to_string()),
            frame_index: Some(0),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: ArtifactKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }

    #[test]
    fn image_roundtrip_minimal() {
        let kind = ArtifactKind::Image {
            width: None,
            height: None,
            format: None,
            frame_index: None,
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: ArtifactKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
    }

    #[test]
    fn image_serialized_has_type_tag() {
        let kind = ArtifactKind::Image {
            width: Some(1920),
            height: Some(1080),
            format: Some("EXR".to_string()),
            frame_index: Some(5),
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("\"type\":\"image\""), "json: {}", json);
        assert!(json.contains("1920"));
        assert!(json.contains("1080"));
        assert!(json.contains("EXR"));
        assert!(json.contains("5"));
    }

    #[test]
    fn image_minimal_omits_none_fields() {
        let kind = ArtifactKind::Image {
            width: None,
            height: None,
            format: None,
            frame_index: None,
        };
        let json = serde_json::to_string(&kind).unwrap();
        // Only the type tag should appear; all None fields are skipped.
        assert_eq!(json, r#"{"type":"image"}"#, "json: {}", json);
    }

    #[test]
    fn is_image() {
        let kind = ArtifactKind::Image {
            width: None,
            height: None,
            format: None,
            frame_index: None,
        };
        assert!(kind.is_image());
    }

    #[test]
    fn display_label_with_format() {
        let kind = ArtifactKind::Image {
            width: None,
            height: None,
            format: Some("PNG".to_string()),
            frame_index: None,
        };
        assert_eq!(kind.display_label(), "PNG image");
    }

    #[test]
    fn display_label_no_format() {
        let kind = ArtifactKind::Image {
            width: None,
            height: None,
            format: None,
            frame_index: None,
        };
        assert_eq!(kind.display_label(), "image");
    }
}
