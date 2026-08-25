# SSH Server Manager

A Linux desktop app (Tauri 2 + React) for managing SSH servers.

## Features

- **Hosts** — store servers with per-host auth (SSH agent, private key, or password). Passwords and key passphrases live in an encrypted vault (XChaCha20-Poly1305, Argon2-derived key, 0600 key file).
- **Terminal** — tabbed xterm.js terminals over SSH PTY channels.
- **Files** — SFTP browser: navigate, upload/download with progress, rename, delete (recursive), new folders.
- **Port Forwarding** — local (-L), remote (-R), and dynamic SOCKS5 (-D) tunnels.
- **Health** — reachability + latency checks, and (when connected) uptime, load, memory, and disk metrics with sparklines. Optional per-host auto-monitoring.
- **Jobs** — background transfers and tasks with live progress and cancellation.
- Apple-HIG styling with three skins (Apple, Cyberpunk, XP) and light/dark themes, all driven by CSS design tokens; every widget (dropdowns, checkboxes, toggles, segmented controls) is custom, never native.

The SSH stack is pure Rust ([russh](https://crates.io/crates/russh) + russh-sftp) in the `ssh-core` crate — no system OpenSSH/libssh dependency.

## Development

Requires Rust, Node 18+, and WebKitGTK 4.1 dev packages
(`webkit2gtk4.1-devel gtk3-devel libsoup3-devel` on Fedora).

```sh
npm install
npm run tauri dev     # run the app
npm test              # frontend tests (vitest)
cargo test -p ssh-core  # backend tests (in-process SSH server, no sshd needed)
npm run tauri build   # produce deb/rpm/AppImage
```

Building the binary directly with cargo? Debug builds load the Vite dev server
(run `npm run dev` alongside); for a standalone binary use
`cargo build --release -p agentmux-ssh --features custom-protocol`
(the Tauri CLI passes that feature automatically).

## Tests

`crates/ssh-core/tests` spins up an in-process SSH server (russh server side)
and exercises the real client stack end to end:

- **networking** — connect, auth success/failure, exec, unreachable hosts
- **terminals** — PTY echo round-trips, resize, exit events (direct + via the manager event bus)
- **credentials** — vault round-trip, encryption at rest, wrong-key fails closed, key-file permissions, host store CRUD
- **SFTP** — browse/mkdir/rename/recursive delete, upload/download as jobs with progress
- **forwarding** — local, remote, and SOCKS5 tunnels end to end
- **health** — probe parsing, metrics over a live session, unreachable hosts
- **jobs** — lifecycle, progress, failure, cancellation
