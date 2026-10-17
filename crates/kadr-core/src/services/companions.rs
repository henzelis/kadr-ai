//! Keeps related files together: sidecars (AAE, XMP, THM) and the video half
//! of Live Photos belong to their main photo. They are hidden from the
//! gallery as separate items and will travel with the photo when sorting.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::media_item::MediaKind;

/// A Live Photo video is a few seconds long.
const LIVE_VIDEO_MAX_MS: u64 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionRole {
    Sidecar,
    LiveVideo,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub path: String,
    pub kind: MediaKind,
    pub duration_ms: Option<u64>,
    /// Apple's ContentIdentifier, shared by both halves of a Live Photo.
    pub content_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub child: i64,
    pub parent: i64,
    pub role: CompanionRole,
}

/// Directory and file stem used to group related files. `IMG_1.JPG.xmp`
/// (darktable style) groups with `IMG_1.JPG` as well as with `IMG_1`.
fn keys(path: &str) -> (String, String, Option<String>) {
    let p = Path::new(path);
    let dir = p
        .parent()
        .map(|d| d.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    // For "img_1.jpg" (stem of "IMG_1.JPG.xmp") also try "img_1".
    let inner = Path::new(&stem)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| *s != stem);
    (dir, stem, inner)
}

/// Decides which files are companions of which. Pure: no I/O.
pub fn find_links(items: &[Candidate]) -> Vec<Link> {
    let mut groups: HashMap<(String, String), Vec<&Candidate>> = HashMap::new();
    for item in items {
        let (dir, stem, _) = keys(&item.path);
        groups.entry((dir, stem)).or_default().push(item);
    }
    let mut links = Vec::new();
    for item in items {
        let (dir, stem, inner) = keys(&item.path);
        let parent_of = |group: Option<&Vec<&Candidate>>| -> Option<i64> {
            let group = group?;
            group
                .iter()
                .filter(|c| c.id != item.id)
                .find(|c| matches!(c.kind, MediaKind::Photo | MediaKind::RawPhoto))
                .or_else(|| {
                    group
                        .iter()
                        .filter(|c| c.id != item.id)
                        .find(|c| c.kind == MediaKind::Video)
                })
                .map(|c| c.id)
        };
        match item.kind {
            MediaKind::Sidecar => {
                let parent = parent_of(groups.get(&(dir.clone(), stem.clone())))
                    .or_else(|| inner.and_then(|i| parent_of(groups.get(&(dir, i)))));
                if let Some(parent) = parent {
                    links.push(Link {
                        child: item.id,
                        parent,
                        role: CompanionRole::Sidecar,
                    });
                }
            }
            MediaKind::Video => {
                let Some(group) = groups.get(&(dir, stem)) else {
                    continue;
                };
                let photo = group
                    .iter()
                    .find(|c| c.kind == MediaKind::Photo && c.id != item.id);
                if let Some(photo) = photo {
                    let same_id = matches!((&item.content_id, &photo.content_id), (Some(a), Some(b)) if a == b);
                    let short = item.duration_ms.is_some_and(|d| d <= LIVE_VIDEO_MAX_MS);
                    let other_id = matches!((&item.content_id, &photo.content_id), (Some(a), Some(b)) if a != b);
                    if same_id || (short && !other_id) {
                        links.push(Link {
                            child: item.id,
                            parent: photo.id,
                            role: CompanionRole::LiveVideo,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: i64, path: &str, kind: MediaKind) -> Candidate {
        Candidate {
            id,
            path: path.to_owned(),
            kind,
            duration_ms: None,
            content_id: None,
        }
    }

    #[test]
    fn sidecars_follow_their_photo() {
        let items = [
            c(1, "/a/IMG_0001.HEIC", MediaKind::Photo),
            c(2, "/a/IMG_0001.AAE", MediaKind::Sidecar),
            c(3, "/a/DSC_1.NEF", MediaKind::RawPhoto),
            c(4, "/a/DSC_1.NEF.xmp", MediaKind::Sidecar),
            c(5, "/a/lonely.xmp", MediaKind::Sidecar),
            c(6, "/b/IMG_0001.AAE", MediaKind::Sidecar),
        ];
        let links = find_links(&items);
        assert!(links.contains(&Link {
            child: 2,
            parent: 1,
            role: CompanionRole::Sidecar
        }));
        assert!(links.contains(&Link {
            child: 4,
            parent: 3,
            role: CompanionRole::Sidecar
        }));
        // No parent in its folder: stays on its own.
        assert!(!links.iter().any(|l| l.child == 5 || l.child == 6));
    }

    #[test]
    fn live_photo_video_by_content_id_or_short_clip() {
        let mut v = c(2, "/a/IMG_0002.MOV", MediaKind::Video);
        v.content_id = Some("ABC".into());
        v.duration_ms = Some(9000);
        let mut p = c(1, "/a/IMG_0002.HEIC", MediaKind::Photo);
        p.content_id = Some("ABC".into());
        assert_eq!(
            find_links(&[p, v]),
            [Link {
                child: 2,
                parent: 1,
                role: CompanionRole::LiveVideo
            }]
        );

        let mut short = c(4, "/a/IMG_0003.MOV", MediaKind::Video);
        short.duration_ms = Some(2900);
        assert_eq!(
            find_links(&[c(3, "/a/img_0003.jpg", MediaKind::Photo), short]).len(),
            1
        );
    }

    #[test]
    fn real_videos_are_not_swallowed() {
        let mut long = c(2, "/a/IMG_0004.MOV", MediaKind::Video);
        long.duration_ms = Some(60_000);
        assert!(find_links(&[c(1, "/a/IMG_0004.JPG", MediaKind::Photo), long]).is_empty());
        // Different Live Photo identifiers: unrelated even if short.
        let mut v = c(4, "/a/IMG_0005.MOV", MediaKind::Video);
        v.duration_ms = Some(2000);
        v.content_id = Some("X".into());
        let mut p = c(3, "/a/IMG_0005.HEIC", MediaKind::Photo);
        p.content_id = Some("Y".into());
        assert!(find_links(&[p, v]).is_empty());
        // A video alone is just a video.
        assert!(find_links(&[c(9, "/a/clip.mp4", MediaKind::Video)]).is_empty());
    }
}
