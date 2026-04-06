# 🎵 aghani (أغاني)

A fast, feature-rich terminal music player with album art, lyrics, and MPRIS support.

![aghani screenshot](screenshot.png)

## Features

- 🖼️  Album art in the terminal (Kitty/Sixel/half-block)
- 🎨  Dynamic themes extracted from album art
- 📝  Synced lyrics (auto-fetched + manual editing)
- 📚  Library browser: Songs, Albums, Artists, Playlists
- 🔀  Shuffle & repeat modes
- 🔌  MPRIS2 support (media keys, playerctl)
- 🔗  IPC socket for i3blocks/waybar integration
- ⚡  Fast startup with metadata cache
- 🎯  Gapless playback
- 🔊  ReplayGain normalization

## Dependencies

- `ffmpeg` — audio playback and seeking
- `chafa` — cover support
- A terminal with true color support (kitty, wezterm, alacritty, etc.)

## Installation

### From crates.io

```bash
cargo install aghani
```

### From source

```bash
git clone https://github.com/yourusername/aghani
cd aghani
cargo build --release
sudo cp target/release/aghani /usr/local/bin/
```

### AUR (Arch Linux)

```bash
yay -S aghani
```

## Usage

```bash
aghani                    # use music dir from config
aghani ~/Music            # specify music directory
aghani --dir ~/Music      # same with flag
aghani --help             # show help
aghani --caches           # show cache locations
aghani --clear-caches     # clear all caches
```

## Keybinds

| Key | Action |
| ----- | -------- |
| `Space` | Play / Pause |
| `n` / `p` | Next / Previous |
| `Up` / `Down` | Navigate |
| `Enter` | Select / Open |
| `Tab` | Switch tab |
| `/` | Search |
| `s` | Save to playlist |
| `f` | Add to favorites |
| `r` | Toggle shuffle |
| `Shift+R` | Toggle repeat |
| `>` / `<` | Seek ±5s |
| `=` / `-` | Volume |
| `g` | Go to playing track |
| `t` | Go to the top of the list |
| `b` | Go to the bottom of the list |
| `l` | Show lyrics |
| `L` | Edit lyrics |
| `1`–`4` | Switch layout |
| `x` | Back |
| `q` | Quit |

## Configuration

Config file: `~/.config/aghani/conf`

```toml
music_dir = "/home/user/Music"

[colors]
dynamic_theme = true
dynamic_theme_style = "vibrant"  # vibrant | muted | dark | light
accent = "Cyan"
title = "Cyan"

[layout]
mode = "default"  # default | compact | wide | minimal
show_cover = true
show_player = true
cover_width = 40

[keys]
quit = "q"
play_pause = "Space"
next = "n"
prev = "p"
```

## i3blocks Integration

```bash
# ~/.local/bin/aghani-ctl
echo "$1" | socat - UNIX-CONNECT:/tmp/aghani.sock > /dev/null 2>&1

```

```ini
[aghani]
command=echo "status" | socat - UNIX-CONNECT:/tmp/aghani.sock > /dev/null 2>&1; cat /tmp/aghani-status 2>/dev/null
interval=1
```

## License

MIT — see [LICENSE](LICENSE)
