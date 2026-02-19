use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Supported audio file extensions.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg"];

/// A single audio file discovered during a directory scan.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size_bytes: u64,
}

/// Result of scanning a directory for audio files.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub root: PathBuf,
    pub files: Vec<FileEntry>,
}

/// Errors that can occur during scanning.
#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    NotADirectory,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::NotADirectory => write!(f, "path is not a directory"),
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScanError::Io(e) => Some(e),
            ScanError::NotADirectory => None,
        }
    }
}

impl From<io::Error> for ScanError {
    fn from(e: io::Error) -> Self {
        ScanError::Io(e)
    }
}

/// Scan `dir` for audio files matching the given extensions (case-insensitive).
///
/// Returns files sorted alphabetically by name. Non-recursive — only scans the
/// immediate directory, not subdirectories. Files that cannot be read (permission
/// errors, broken symlinks) are silently skipped.
pub fn scan_directory(dir: &Path, extensions: &[&str]) -> Result<ScanResult, ScanError> {
    if !dir.is_dir() {
        return Err(ScanError::NotADirectory);
    }

    let ext_lower: Vec<String> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();

    let mut files = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if !metadata.is_file() {
            continue;
        }

        let path = entry.path();

        let ext = path
            .extension()
            .and_then(OsStr::to_str)
            .map(|s| s.to_ascii_lowercase());

        let ext = match ext {
            Some(e) if ext_lower.contains(&e) => e,
            _ => continue,
        };

        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_string();

        files.push(FileEntry {
            path,
            name,
            extension: ext,
            size_bytes: metadata.len(),
        });
    }

    files.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    Ok(ScanResult {
        root: dir.to_path_buf(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("kick.wav"), b"fake wav").unwrap();
        fs::write(dir.path().join("snare.flac"), b"fake flac").unwrap();
        fs::write(dir.path().join("hihat.mp3"), b"fake mp3").unwrap();
        fs::write(dir.path().join("pad.ogg"), b"fake ogg").unwrap();
        fs::write(dir.path().join("readme.txt"), b"not audio").unwrap();
        fs::write(dir.path().join("data.json"), b"not audio").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        dir
    }

    #[test]
    fn scan_finds_supported_files() {
        let dir = setup_test_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert_eq!(result.files.len(), 4);
    }

    #[test]
    fn scan_excludes_non_audio() {
        let dir = setup_test_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        let names: Vec<&str> = result.files.iter().map(|f| f.name.as_str()).collect();
        assert!(!names.contains(&"readme.txt"));
        assert!(!names.contains(&"data.json"));
    }

    #[test]
    fn scan_excludes_directories() {
        let dir = setup_test_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        let names: Vec<&str> = result.files.iter().map(|f| f.name.as_str()).collect();
        assert!(!names.contains(&"subdir"));
    }

    #[test]
    fn scan_sorts_alphabetically() {
        let dir = setup_test_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        let names: Vec<&str> = result.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["hihat.mp3", "kick.wav", "pad.ogg", "snare.flac"]
        );
    }

    #[test]
    fn scan_case_insensitive_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.WAV"), b"data").unwrap();
        fs::write(dir.path().join("test.Mp3"), b"data").unwrap();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert_eq!(result.files.len(), 2);
    }

    #[test]
    fn scan_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert!(result.files.is_empty());
    }

    #[test]
    fn scan_not_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, b"data").unwrap();
        let result = scan_directory(&file_path, SUPPORTED_EXTENSIONS);
        assert!(matches!(result, Err(ScanError::NotADirectory)));
    }

    #[test]
    fn scan_nonexistent_path() {
        let result = scan_directory(Path::new("/nonexistent/path"), SUPPORTED_EXTENSIONS);
        assert!(result.is_err());
    }

    #[test]
    fn scan_records_root() {
        let dir = setup_test_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert_eq!(result.root, dir.path());
    }

    #[test]
    fn file_entry_has_correct_metadata() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.wav"), b"12345").unwrap();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert_eq!(result.files.len(), 1);
        let entry = &result.files[0];
        assert_eq!(entry.name, "test.wav");
        assert_eq!(entry.extension, "wav");
        assert_eq!(entry.size_bytes, 5);
    }

    #[test]
    fn scan_with_custom_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.wav"), b"data").unwrap();
        fs::write(dir.path().join("b.mp3"), b"data").unwrap();
        let result = scan_directory(dir.path(), &["wav"]).unwrap();
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].extension, "wav");
    }
}
