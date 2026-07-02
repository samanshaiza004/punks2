# punks2 devlog #1 — Rebuilding a sample browser in Rust

*Draft / outline — 2026-06-22*

## What punks2 is

punks2 is a modular **sample browser** for musicians: open a folder of audio
files, step through them, hear each one instantly, and drag the one you want
into your DAW. It's a small, fast, native desktop app (~3k lines of Rust across
five crates).

It's also the **foundation of a larger project**. punks2 is the first module of
a future Rust DAW — the browser is being built first, standalone, so the audio,
navigation, and persistence layers can be hardened in isolation and then
embedded into the bigger app as a library. The guiding rule throughout is *each
layer owns exactly what belongs to it*.

## Goals

- **Instant, frictionless auditioning.** Click or keyboard-walk through a folder
  and hear samples with no perceptible delay.
- **Real-time-safe audio.** The output path never blocks, never allocates on the
  audio thread, and never glitches under load.
- **Reusable by design.** The browser and playback engine are libraries with no
  UI dependencies, so the DAW can embed them directly.
- **Small and boring.** Smallest correct solution; few dependencies, few
  abstractions, no runtime to ship.
- **Cross-platform.** macOS, Linux, Windows from one codebase.

## How it's made

A Rust workspace of five crates, layered so dependencies only ever point
"downward":

```
punks-core  →  punks-playback  →  punks-browser  →  punks-ui  →  punks-standalone
```

- **`punks-core`** — directory listing and JSON config/persistence
  (`serde`, `dirs`). No audio, no UI. Pure, well-tested.
- **`punks-playback`** — the audio engine. `cpal` for native output
  (CoreAudio / WASAPI / ALSA), `symphonia` for decoding WAV/FLAC/MP3/OGG,
  `rubato` for sample-rate conversion, and an `lru` decode cache for instant
  replay. The audio callback is **lock-free**: state lives in atomics and an
  `RwLock` the callback only ever `try_read`s, degrading to silence rather than
  blocking. Decoding happens on a background thread; a `Release`/`Acquire` pair
  hands the finished buffer to the callback safely.
- **`punks-browser`** — navigation as a domain model: directory history,
  selection, threaded recursive search, and now multi-tab state. Wraps the
  playback engine. Still no UI dependency.
- **`punks-ui`** — an immediate-mode GUI built on `imgui`, plus `rfd` for the
  native folder picker. One `BrowserPanel::draw` per frame.
- **`punks-standalone`** — the shell: a `winit` window, `wgpu` + `imgui-wgpu`
  render loop, and native drag-out (`drag`) so you can drag a sample straight
  into another app.

## Why it's an upgrade from the original Electron codebase

punks2 is a ground-up rewrite of an earlier Electron version. The move to native
Rust is mostly about what an *audio* app needs:

- **No garbage collector in the audio path.** A GC pause during playback is an
  audible glitch. Rust lets the output callback be fully lock-free and
  allocation-free — something that's structurally hard in a JS/Electron runtime.
- **Direct hardware audio.** `cpal` talks to the OS audio APIs in-process,
  instead of going through Web Audio and a Chromium sandbox.
- **No runtime to ship.** No bundled Chromium + Node. The result is a single
  small native binary with fast startup and low memory, instead of a
  hundreds-of-megabytes app that boots a browser to draw a list.
- **In-process decoding.** `symphonia` decodes on a worker thread with no IPC
  bridge between a renderer and a main process.
- **Reusable, enforced boundaries.** The Rust crate layering makes the browser
  and engine embeddable libraries with compiler-checked module boundaries — the
  DAW will `use punks_browser::SampleBrowser` directly. The Electron structure
  didn't give us that seam.

*(Honest caveat: this section is the rationale for going native, grounded in the
current architecture — not a feature-by-feature diff against the old app.)*

## Log — what's been done so far

**Foundation**
- Layered workspace; core listing + config; lock-free playback engine;
  background decode thread + LRU cache; imgui UI; winit/wgpu standalone shell.
- Waveform visualizer with playhead; remappable keybinds; configurable samples
  folder; native drag-out.

**Volume + persistence**
- Working preview volume slider, persisted across sessions.
- Fixed directory persistence to restore the *exact* subdirectory you left off
  in (previously only saved the folder you picked, not where you navigated).

**Audit & hardening pass**
- Deleted dead/reserved code; refreshed the README; committed `Cargo.lock`;
  added a `fmt + clippy + test` CI workflow (Linux + macOS).
- Audio `Release`/`Acquire` ordering on the buffer swap; poisoned-lock recovery;
  search errors logged instead of swallowed; proper stereo→mono downmix
  (average, not truncate).
- Performance: switched the file list to an imgui `ListClipper`, eliminating a
  per-frame allocation of the entire listing.

**Tabs**
- Full multi-tab navigation: each tab has its own history, selection, and search
  state; one global playback engine shared across tabs. Drag-to-reorder, a
  custom tab bar (imgui can't read its native tab reorder back), close with
  min-one-tab, and next/prev/new/close keybinds.

**Decode robustness**
- Handle Ogg-Vorbis-in-WAV files (WAVE format tag `0x674f`, from the Vorbis ACM
  codec) by extracting the inner Ogg stream from the `data` chunk — a real
  format that shows up in older sample packs.

**UI polish**
- Browser-like tabs (active accent vs. muted inactive, attached close glyph);
  width-adaptive multi-column file list; a calmer dark theme.

## What's next

**Near term**
- Tab persistence across launches (restore the open set, not just one folder).
- A single reusable decode worker ("latest request wins") instead of spawning a
  thread per keypress during fast scrolling.
- Broader format coverage (sibling Ogg-in-WAV tags; surface genuinely
  unsupported codecs clearly).

**Browser features**
- Metadata/tags, favorites, and BPM/key detection.
- Waveform zoom, loop region, and quick trim.

**Toward the DAW**
- Define the `AudioSink` seam so the engine can render into a host mix bus, not
  just the default device.
- Embed `punks-browser` into the DAW shell; build out timeline, mixer, and
  plugin hosting on top of the same layered core.
