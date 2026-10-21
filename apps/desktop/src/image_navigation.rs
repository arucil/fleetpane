//! A filename-based index of the already bounded directory observation.
use fleetpane_session::files::{Kind, Listing};
use std::path::Path;

pub struct Position<'a> {
    pub index: usize,
    pub total: usize,
    pub previous: Option<&'a str>,
    pub next: Option<&'a str>,
}

pub fn named_image(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["png", "jpg", "jpeg", "webp"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

pub fn position<'a>(listing: &'a Listing, path: &str) -> Option<Position<'a>> {
    let paths: Vec<_> = listing
        .entries
        .iter()
        .filter(|entry| entry.kind == Kind::File && named_image(&entry.path))
        .map(|entry| entry.path.as_str())
        .collect();
    let index = paths.iter().position(|candidate| *candidate == path)?;
    Some(Position {
        index,
        total: paths.len(),
        previous: index.checked_sub(1).map(|index| paths[index]),
        next: paths.get(index + 1).copied(),
    })
}

pub fn destination(listing: &Listing, path: &str, forward: bool) -> Result<String, String> {
    let position = position(listing, path)
        .ok_or("Current image is absent from the image list; refresh or select a file.")?;
    (if forward {
        position.next
    } else {
        position.previous
    })
    .map(str::to_owned)
    .ok_or_else(|| "No image in that direction; the directory observation changed.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_session::files::Entry;

    fn listing() -> Listing {
        Listing {
            workspace_id: uuid::Uuid::nil(),
            path: "pictures".into(),
            entries: [
                ("folder.png", Kind::Directory),
                ("a.PNG", Kind::File),
                ("b..jpeg", Kind::File),
                ("c.webp", Kind::Symlink),
                ("d.jpg", Kind::Other),
                ("e.txt", Kind::File),
                ("图.webp", Kind::File),
            ]
            .into_iter()
            .map(|(name, kind)| Entry {
                name: name.into(),
                path: format!("pictures/{name}"),
                kind,
                size: 10,
            })
            .collect(),
        }
    }

    #[test]
    fn image_navigation_preserves_directory_order_and_excludes_links_and_special_files() {
        let listing = listing();
        let first = position(&listing, "pictures/a.PNG").unwrap();
        assert_eq!((first.index, first.total, first.previous), (0, 3, None));
        assert_eq!(first.next, Some("pictures/b..jpeg"));
        let last = position(&listing, "pictures/图.webp").unwrap();
        assert_eq!((last.index, last.total, last.next), (2, 3, None));
        assert_eq!(last.previous, Some("pictures/b..jpeg"));
        assert!(position(&listing, "pictures/c.webp").is_none());
        assert_eq!(
            destination(&listing, "pictures/b..jpeg", true).unwrap(),
            "pictures/图.webp"
        );
        assert!(destination(&listing, "pictures/a.PNG", false).is_err());
    }

    #[test]
    fn image_navigation_uses_fresh_membership_and_reports_missing_current_image() {
        let mut listing = listing();
        assert_eq!(
            destination(&listing, "pictures/a.PNG", true).unwrap(),
            "pictures/b..jpeg"
        );
        listing.entries.retain(|entry| entry.name != "b..jpeg");
        assert_eq!(
            destination(&listing, "pictures/a.PNG", true).unwrap(),
            "pictures/图.webp"
        );
        assert!(
            destination(&listing, "pictures/b..jpeg", true)
                .unwrap_err()
                .contains("absent")
        );
        listing.entries.clear();
        assert!(position(&listing, "pictures/a.PNG").is_none());
    }

    #[test]
    fn image_navigation_does_not_guess_other_formats_or_extensionless_files() {
        for path in ["PNG", "image", ".png", "a.svg", "a.gif", "a.png.txt"] {
            assert!(!named_image(path), "{path}");
        }
        for path in ["图.PNG", "a..JpG", "a.jpeg", "a.WebP"] {
            assert!(named_image(path), "{path}");
        }
    }
}
