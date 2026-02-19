# punks2

A modular sample browser for musicians, built in Rust. Designed as the first standalone module for a future open-source DAW.

## What it does

- Scan a directory for audio files (WAV, FLAC, MP3, OGG)
- Browse files in a scrollable list
- Click to preview-play through your default audio device
- Stop playback

## Architecture

Five crates, one responsibility each:

| Crate | Role |
|---|---|
| `punks-core` | File scanning, types, metadata — zero external deps |
| `punks-playback` | Audio decode (symphonia) + output (cpal) + resampling (rubato) |
| `punks-browser` | Module facade — composes core + playback behind a clean API |
| `punks-ui` | imgui rendering — no window ownership, embeddable in any imgui host |
| `punks-standalone` | Binary — winit + wgpu + imgui bootstrap |

The DAW integration path: depend on `punks-browser` + `punks-ui`, create a `SampleBrowser`, call `panel.draw(&ui, &mut browser)` inside your imgui frame.

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

MIT OR Apache-2.0
