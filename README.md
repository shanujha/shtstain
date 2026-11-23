<div align="center">

# sh*tstain

**See how much disk space your projects (and their dependency junk) are really eating.**

[![Release](https://img.shields.io/github/v/release/shanujha/shtstain?style=for-the-badge&color=8a5a2b)](https://github.com/shanujha/shtstain/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/shanujha/shtstain/total?style=for-the-badge&color=8a5a2b)](https://github.com/shanujha/shtstain/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
![Platform](https://img.shields.io/badge/platform-Windows-0078D6?style=for-the-badge&logo=windows)
![Made with AI](https://img.shields.io/badge/made%20with-AI-8A2BE2?style=for-the-badge)

</div>

> 🤖 **Made with AI to quickly solve one problem: reclaiming disk space on small-storage laptops.**

## What it does

- 📁 **Watch project folders**: add single projects, or tick *"Folder contains projects"* to add every subfolder of a parent folder as its own project.
- 📦 **Separates source from dependencies**: `node_modules`, Python virtualenvs (`.venv`, `venv`, anything with `pyvenv.cfg`), `__pycache__`, Rust `target`, `vendor`, `.next`, `.gradle`, `Pods` and more.
- 📊 **Table view**: per-project source, dependency and total size; expand a project to see each dependency folder.
- 🧹 **One-click cleanup**: delete a project's dependencies (or a single dependency folder) after a confirmation. They can be reinstalled later.
- 🔄 **Auto rescan** every N seconds (default 60).
- 🔔 **System tray**: left-click to open, right-click for the current occupied size, *Open* and *Exit*. Closing the window hides it to the tray.
- 🪶 **Tiny & fast**: a single ~4 MB native executable, no runtime, no installer.

## Install

Download `shtstain.exe` from the [latest release](https://github.com/shanujha/shtstain/releases/latest) and run it.

Or build from source:

```sh
git clone https://github.com/shanujha/shtstain
cd shtstain
cargo build --release
# target\release\shtstain.exe
```

## Notes

- Settings are stored in `%APPDATA%\shtstain\config.json`.
- Deleting dependencies is permanent (it does not use the Recycle Bin).
- The list of folders treated as dependencies is the `DEP_DIRS` constant in `src/main.rs`.

## Built with

[egui / eframe](https://github.com/emilk/egui), [tray-icon](https://github.com/tauri-apps/tray-icon), [rfd](https://github.com/PolyMeilex/rfd).

## License

[MIT](LICENSE)
