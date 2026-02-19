use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg"];

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone)]
pub struct DirListing {
    pub root: PathBuf,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub root: PathBuf,
    pub files: Vec<FileEntry>,
}

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

pub fn list_directory(dir: &Path) -> Result<DirListing, ScanError> {
    if !dir.is_dir() {
        return Err(ScanError::NotADirectory);
    }

    let mut dirs: Vec<FileEntry> = Vec::new();
    let mut files: Vec<FileEntry> = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().into_owned();

        if name.starts_with('.') {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let path = entry.path();

        if metadata.is_dir() {
            dirs.push(FileEntry {
                path,
                name,
                extension: String::new(),
                size_bytes: 0,
                is_directory: true,
            });
        } else if metadata.is_file() {
            let ext = path
                .extension()
                .and_then(OsStr::to_str)
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                files.push(FileEntry {
                    name,
                    extension: ext,
                    size_bytes: metadata.len(),
                    path,
                    is_directory: false,
                });
            }
        }
    }

    let ci_sort = |a: &FileEntry, b: &FileEntry| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    };
    dirs.sort_by(ci_sort);
    files.sort_by(ci_sort);

    let mut entries = dirs;
    entries.extend(files);

    Ok(DirListing {
        root: dir.to_path_buf(),
        entries,
    })
}

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
            is_directory: false,
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

    fn make_audio_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("kick.wav"), b"fake wav").unwrap();
        fs::write(dir.path().join("snare.flac"), b"fake flac").unwrap();
        fs::write(dir.path().join("hihat.mp3"), b"fake mp3").unwrap();
        fs::write(dir.path().join("pad.ogg"), b"fake ogg").unwrap();
        fs::write(dir.path().join("readme.txt"), b"not audio").unwrap();
        fs::write(dir.path().join("data.json"), b"not audio").unwrap();
        fs::create_dir(dir.path().join("Loops")).unwrap();
        fs::create_dir(dir.path().join("One-Shots")).unwrap();
        dir
    }

    #[test]
    fn list_includes_subdirs_and_audio_files() {
        let dir = make_audio_dir();
        let result = list_directory(dir.path()).unwrap();
        assert_eq!(result.entries.len(), 6);
    }

    #[test]
    fn list_dirs_appear_before_files() {
        let dir = make_audio_dir();
        let result = list_directory(dir.path()).unwrap();
        let is_dir_flags: Vec<bool> = result.entries.iter().map(|e| e.is_directory).collect();
        let first_file = is_dir_flags.iter().position(|&d| !d).unwrap_or(0);
        assert!(is_dir_flags[..first_file].iter().all(|&d| d));
        assert!(is_dir_flags[first_file..].iter().all(|&d| !d));
    }

    #[test]
    fn list_dirs_sorted_alphabetically() {
        let dir = make_audio_dir();
        let result = list_directory(dir.path()).unwrap();
        let dir_names: Vec<&str> = result
            .entries
            .iter()
            .filter(|e| e.is_directory)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(dir_names, vec!["Loops", "One-Shots"]);
    }

    #[test]
    fn list_files_sorted_alphabetically() {
        let dir = make_audio_dir();
        let result = list_directory(dir.path()).unwrap();
        let file_names: Vec<&str> = result
            .entries
            .iter()
            .filter(|e| !e.is_directory)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            file_names,
            vec!["hihat.mp3", "kick.wav", "pad.ogg", "snare.flac"]
        );
    }

    #[test]
    fn list_excludes_non_audio_files() {
        let dir = make_audio_dir();
        let result = list_directory(dir.path()).unwrap();
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&"readme.txt"));
        assert!(!names.contains(&"data.json"));
    }

    #[test]
    fn list_excludes_hidden_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("kick.wav"), b"data").unwrap();
        fs::write(dir.path().join(".hidden.wav"), b"data").unwrap();
        fs::create_dir(dir.path().join(".hiddendir")).unwrap();
        let result = list_directory(dir.path()).unwrap();
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.iter().any(|n| n.starts_with('.')));
        assert_eq!(result.entries.len(), 1);
    }

    #[test]
    fn list_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let result = list_directory(dir.path()).unwrap();
        assert!(result.entries.is_empty());
    }

    #[test]
    fn list_not_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.wav");
        fs::write(&file, b"data").unwrap();
        assert!(matches!(
            list_directory(&file),
            Err(ScanError::NotADirectory)
        ));
    }

    #[test]
    fn list_nonexistent_path() {
        assert!(list_directory(Path::new("/nonexistent/path/xyz")).is_err());
    }

    #[test]
    fn list_dir_entry_has_correct_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("drums")).unwrap();
        let result = list_directory(dir.path()).unwrap();
        let entry = &result.entries[0];
        assert_eq!(entry.name, "drums");
        assert!(entry.is_directory);
        assert_eq!(entry.size_bytes, 0);
        assert!(entry.extension.is_empty());
    }

    #[test]
    fn list_file_entry_has_correct_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("kick.wav"), b"12345").unwrap();
        let result = list_directory(dir.path()).unwrap();
        let entry = &result.entries[0];
        assert_eq!(entry.name, "kick.wav");
        assert!(!entry.is_directory);
        assert_eq!(entry.extension, "wav");
        assert_eq!(entry.size_bytes, 5);
    }

    #[test]
    fn list_case_insensitive_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.WAV"), b"data").unwrap();
        fs::write(dir.path().join("test.Mp3"), b"data").unwrap();
        let result = list_directory(dir.path()).unwrap();
        assert_eq!(result.entries.len(), 2);
        assert!(result.entries.iter().all(|e| !e.is_directory));
    }

    #[test]
    fn scan_finds_supported_files() {
        let dir = make_audio_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert_eq!(result.files.len(), 4);
    }

    #[test]
    fn scan_excludes_non_audio() {
        let dir = make_audio_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        let names: Vec<&str> = result.files.iter().map(|f| f.name.as_str()).collect();
        assert!(!names.contains(&"readme.txt"));
        assert!(!names.contains(&"data.json"));
    }

    #[test]
    fn scan_excludes_directories() {
        let dir = make_audio_dir();
        let result = scan_directory(dir.path(), SUPPORTED_EXTENSIONS).unwrap();
        assert!(result.files.iter().all(|f| !f.is_directory));
    }

    #[test]
    fn scan_sorts_alphabetically() {
        let dir = make_audio_dir();
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
        assert!(matches!(
            scan_directory(&file_path, SUPPORTED_EXTENSIONS),
            Err(ScanError::NotADirectory)
        ));
    }

    #[test]
    fn scan_nonexistent_path() {
        assert!(scan_directory(Path::new("/nonexistent/path"), SUPPORTED_EXTENSIONS).is_err());
    }

    #[test]
    fn scan_records_root() {
        let dir = make_audio_dir();
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
        assert!(!entry.is_directory);
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
