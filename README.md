# local-transcriber

Local, CPU-based speech-to-text for desktop Linux and Windows.

Each target is a self-contained application so platform dependencies, build
artifacts, packaging, and documentation cannot drift across operating systems.

| Target | Project | Getting started |
|---|---|---|
| Linux | [`linux/`](linux/) | [`linux/README.md`](linux/README.md) |
| Windows | [`windows/`](windows/) | [`windows/README.md`](windows/README.md) |

Run Cargo and platform scripts from the target directory you are working on.

## 👏 Credits & Acknowledgments

This project is directly derived from and inspired by the excellent work of **[FirePheonix](https://github.com/FirePheonix)**.

While this repository was re-structured as a standalone project rather than a GitHub fork, it relies heavily on their original CPU-optimized implementation:

* **Original Concept & Base Code:** [FirePheonix/parakeet-tdt-v3-CPU-optimized](https://github.com/FirePheonix/parakeet-tdt-v3-CPU-optimized)
* **Architecture & Writeup:** [Shubham's Blog: How I ran Parakeet STT on CPU](https://blogs.shubhamz.dev/systems/how-i-ran-parakeet-stt-on-cpu)

### Key Differences in This Repository:
* 🐧 Added a dedicated, native **Linux target** and build workflow.
* 🪟 Restructured and updated the **Windows target** to align with the shared desktop layout.
* ✨ Added custom feature tweaks and optimizations (auto paste after transcribing, select mic device, sha256 model check, hide notifications).
