# punks2

A modular sample browser for musicians, built in Rust.

## What it does

- Browse directories of audio files (WAV, FLAC, MP3, OGG) with breadcrumb navigation
- Preview-play through your default audio device — click a file, or use keyboard
  navigation (W/S or arrow keys) to step through and auto-play
- Instant replay from an in-memory decode cache when you revisit a sample
- Volume control for previews, persisted across sessions
- Recursive filename search from the current directory
- Waveform visualizer with a playhead
- Remappable keybinds and a configurable samples folder via the Settings modal
- Restores the exact directory you left off in on next launch
- Drag a sample out of the browser into another application (macOS/Windows)

## Building

```
cargo build --release -p punks-standalone
```

### Requirements

- Rust 1.84+ (stable)
- macOS, Linux, or Windows
- On Linux: ALSA development libraries (`libasound2-dev` on Debian/Ubuntu)

## Running

```
cargo run -p punks-standalone
```

Click **Browse...** to open a directory, then click any file to preview it.

## Using as a library

Add to your `Cargo.toml`:

```toml
[dependencies]
punks-browser = { path = "crates/punks-browser" }
punks-ui = { path = "crates/punks-ui" }  # if using imgui
```

```rust
use punks_browser::SampleBrowser;

let mut browser = SampleBrowser::new()?;
browser.open_directory(Path::new("/path/to/samples"))?;
browser.select(0);
browser.play_selected();
// ...
browser.stop();
```

## License

[MIT](LICENSE)
