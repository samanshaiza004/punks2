# punks2

A modular sample browser for musicians, built in Rust.

## What it does

- Scan a directory for audio files (WAV, FLAC, MP3, OGG)
- Browse files in a scrollable list
- Click to preview-play through your default audio device
- Stop playback

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
browser.play_selected()?;
// ...
browser.stop();
```

## License

MIT
