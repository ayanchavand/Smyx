# smyx

[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)
[![Crates.io Version](https://img.shields.io/crates/v/smyx?style=for-the-badge)](https://crates.io/crates/smyx)
![GitHub Release](https://img.shields.io/github/v/release/ayanchavand/Smyx?style=for-the-badge)
![Last Commit](https://img.shields.io/github/last-commit/ayanchavand/Smyx?style=for-the-badge)

A lean, beautiful terminal Navidrome / OpenSubsonic player in Rust. Features
album-art-reactive theming, a live audio visualizer, and synced lyrics.

<p align="center"><img src="assets/preview.png" alt="smyx recolors the whole interface to the album art" width="100%"></p>

<p align="center">
  <img src="assets/theme-1.png" width="32%">
  <img src="assets/theme-2.png" width="32%">
  <img src="assets/theme-3.png" width="32%">
</p>

> Requires a **Navidrome** or **OpenSubsonic** compatible server. Works on Linux, macOS, and Windows. Album art is
> crispest on Kitty, WezTerm, or foot.

## Install

Install via [Cargo](https://crates.io) (all platforms — Linux, macOS, Windows):

```bash
cargo install smyx
```

Or build from source:

```bash
git clone https://github.com/ayanchavand/Smyx.git
cd Smyx
cargo install --path .
```

## Get started

Run `smyx` in your terminal:

```bash
smyx
```

On first launch, an interactive login modal will prompt you for your Navidrome / OpenSubsonic server URL, username, and password.

Configurations are automatically saved to `~/.config/smyx/navidrome.toml` (or `%USERPROFILE%\.config\smyx\navidrome.toml` on Windows).

## Keys

```
⇥ / [ ]    switch section        ⇧⇥       switch view
↑↓ / j k   move                  ⏎        play / open
/          search                a        actions
space      play · pause          n / b    next · prev
← →        seek                  s        shuffle
+ / -      volume                R        repeat
o          sort                  r        reload
t          theme                 q        quit
```

Media keys (Play/Pause, Stop, Next, Prev, Volume) work when the terminal is
focused. Mouse works too: click tabs, click a track, double-click to play.

## Credits

`smyx` is a fork of [Myx](https://github.com/HaseebKhalid1507/Myx), which was originally a TUI for Spotify client.

Built on [ratatui](https://ratatui.rs), [rodio](https://github.com/RustAudio/rodio), and [rustfft](https://github.com/ejmahler/RustFFT). Visual language inspired by [noodle](https://github.com/wilfredinni/noodle).
See [NOTICE](NOTICE).

## License

MIT, see [LICENSE](LICENSE).

