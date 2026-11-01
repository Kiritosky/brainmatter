//! Bookmarks: how macOS refers to a file so that it is found again after being
//! renamed or moved. The app is not sandboxed, so no security scope is involved.

use std::path::{Path, PathBuf};

use objc2::runtime::Bool;
use objc2_foundation::{
    NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions,
};

/// `None` if the file does not exist or cannot be bookmarked.
pub fn create(path: &Path) -> Option<Vec<u8>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::empty(),
            None,
            None,
        )
        .ok()?;
    Some(data.to_vec())
}

/// Where a bookmarked file is now; `None` if it is gone. Never shows a dialog or
/// mounts a volume, because this runs in the background.
pub fn resolve(bookmark: &[u8]) -> Option<PathBuf> {
    let data = NSData::with_bytes(bookmark);
    let mut stale = Bool::NO;
    // SAFETY: `stale` is a valid pointer for the duration of the call.
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data,
            NSURLBookmarkResolutionOptions::WithoutUI
                | NSURLBookmarkResolutionOptions::WithoutMounting,
            None,
            &mut stale,
        )
    }
    .ok()?;
    let path = PathBuf::from(url.path()?.to_string());
    path.exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn follows_a_moved_file_and_notices_a_deleted_one() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().canonicalize().unwrap();
        let first = dir.join("vorher.md");
        fs::write(&first, "Inhalt").unwrap();
        let bookmark = create(&first).unwrap();
        assert_eq!(resolve(&bookmark).unwrap(), first);

        fs::create_dir(dir.join("ordner")).unwrap();
        let moved = dir.join("ordner/nachher.md");
        fs::rename(&first, &moved).unwrap();
        assert_eq!(resolve(&bookmark).unwrap(), moved);

        fs::remove_file(&moved).unwrap();
        assert_eq!(resolve(&bookmark), None);
    }

    #[test]
    fn missing_files_and_garbage_are_no_problem() {
        assert_eq!(create(Path::new("/gibt/es/nicht.md")), None);
        assert_eq!(resolve(b"kein bookmark"), None);
    }
}
