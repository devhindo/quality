<div align="center">

# quality

**Make screenshots crisp enough to post.**

[![ci](https://github.com/devhindo/quality/actions/workflows/ci.yml/badge.svg)](https://github.com/devhindo/quality/actions/workflows/ci.yml)
[![release](https://github.com/devhindo/quality/actions/workflows/release.yml/badge.svg)](https://github.com/devhindo/quality/actions/workflows/release.yml)
[![latest](https://img.shields.io/github/v/release/devhindo/quality?label=latest)](https://github.com/devhindo/quality/releases/latest)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

```
quality screenshot.png
```

</div>

---

## Why screenshots go soft when you share them

A screenshot captured on a 1× display contains exactly as many pixels as the region you grabbed — a 296×423 window is 296×423 pixels, and that's all the information there is.

Anywhere that image gets displayed larger than its native size, something has to *upscale* it. That upscaling is almost always bilinear, which is mush: soft text, smeared edges, muddy UI chrome. Zoom in and it falls apart completely.

**Sharpening cannot fix this.** Lanczos, unsharp mask, and every filter of that family only redistribute information already present in the file. 296×423 is ~125,000 pixels of information no matter what you run over it.

`quality` uses super-resolution instead — a model that *reconstructs* plausible detail rather than interpolating between the pixels you have — so the image holds up when it's displayed large or zoomed into. It then fits the result within a size budget so it survives upload without being re-compressed.

---

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/install.sh | sh
```

That installs to `~/.local/bin` for your user only, and never asks for `sudo`.

To install system-wide instead — same script, only the destination differs:

```sh
curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/install.sh | sudo env QUALITY_INSTALL_DIR=/usr/local/bin sh
```

`sudo env VAR=…` rather than `sudo VAR=…`: sudo's default `env_reset` strips
command-line variable assignments it doesn't recognise, so the plain form would
silently fall back to installing under root's home instead. Running `env` as
root sets the variable after sudo, where nothing can filter it.

<details>
<summary><b>Uninstall</b></summary>

```sh
curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/uninstall.sh | sh
```

</details>

<details>
<summary><b>Other install options</b></summary>

**Pin a version**

```sh
curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/install.sh | QUALITY_VERSION=v0.1.0 sh
```

**Windows** — download the `.zip` from [Releases](https://github.com/devhindo/quality/releases/latest), extract, and put `quality.exe` somewhere on your `PATH`.

**From source** (needs Rust 1.85+ for edition 2024)

```sh
git clone https://github.com/devhindo/quality
cd quality
cargo install --path .
```

**Prefer not to pipe the internet into a shell?** Perfectly reasonable:

```sh
curl -fsSLO https://raw.githubusercontent.com/devhindo/quality/main/install.sh
less install.sh    # read it
sh install.sh
```

</details>

### Supported systems

| OS | x86_64 | arm64 |
|---|---|---|
| Linux (glibc 2.38+) | ✅ | ✅ |
| macOS | ⚠️ source only | ✅ Apple Silicon |
| Windows | ✅ | — |

One self-contained binary. The 4.9 MB model is compiled into it — no Python, no ffmpeg, no ONNX Runtime install, no model download on first run.

Two limits, both inherited from the prebuilt ONNX Runtime this links against rather than chosen:

- **glibc 2.38+ on Linux** — Ubuntu 23.10+, Debian 13+, Fedora 39+. The runtime is built against 2.38, so older systems cannot link it. Ubuntu 22.04 is still LTS and is *not* covered; those users need to build from source against a locally built runtime.
- **No Intel Mac binary.** `ort` publishes no prebuilt runtime for `x86_64-apple-darwin` — *"no prebuilt binaries available for target x86_64-apple-darwin"* — so there is nothing to ship. Apple Silicon is fully supported; Intel Mac users must build ONNX Runtime themselves.

---

## Usage

```sh
quality shot.png                # -> shot-quality.webp
quality shot.png --png          # lossless PNG instead
quality shot.png -i 40          # gentler ML effect
quality shot.png -o out.webp    # explicit output path
```

```
input   296x423  (0.13 MP)
upscale 1184x1692  in 3.1s
output  1184x1692  0.09 MB  -> shot-quality.webp
```

### Flags

| Flag | Default | What it does |
|------|---------|--------------|
| `-t, --target` | `x` | Size budget preset to fit within — run `quality --help` for the list |
| `-i, --intensity` | `60` | ML strength, 0–100. Lower is softer and closer to plain Lanczos |
| `-s, --saturation` | `92` | Saturation percent; `100` leaves colour untouched |
| `-q, --quality` | `90` | WebP quality |
| `--png` | off | Write lossless PNG instead of WebP |
| `--tile` | `384` | Inference tile size. Lower it if you run out of memory |
| `-o, --output` | `<input>-quality.webp` | Output path; extension picks the format |

### Fitting a size budget

Upload endpoints reject or silently re-compress images past a certain pixel dimension or file size, which undoes the point of the exercise. `--target` selects a preset pair of caps — a maximum long edge and a maximum file size — and the output is fitted to both. `quality --help` lists the presets; `--target none` disables fitting entirely.

If the encoded file still lands over its size cap, `quality` says so and suggests a lower `--quality` rather than silently handing you something that will be re-compressed.

---

## How it works

**1. Super-resolution at 4×.** Real-ESRGAN `general-x4v3` (SRVGGNetCompact). Images above one tile are processed in 384 px tiles with 24 px of overlap; the overlap is cropped off each result, so no seam ever carries a tile-edge artifact. This bounds peak memory to roughly 1.4 GB — full-frame inference on a 1.55 MP screenshot will OOM a 16 GB machine that's already busy.

**2. Blend back toward Lanczos.** Straight model output looks *wrong* in a specific way: whites read as whiter than they are, and everything looks over-saturated. Measuring it showed mean RGB and mean saturation were **identical** to a Lanczos upscale — the drift was **~29% more local contrast** (std dev 36 vs 28). So the correction is a blend against Lanczos, not a saturation fix. `--intensity` is that blend; the default keeps 60% of the model.

**3. Saturation trim.** A small extra correction for the same effect, at 92%.

**4. Fit and encode.** Downsample to the size budget, then WebP (or PNG). The model always runs at full 4× first even when the budget is smaller — downsampling a 4× result beats upscaling straight to the target size, because it also averages away model artifacts.

### Performance

Roughly 25–30 s/megapixel on CPU. There is no GPU path.

| Input | Output | Time |
|---|---|---|
| 296×423 | 1184×1692 | ~3 s |
| 1780×873 | 4096×2009 | ~50 s |

---

## Things worth knowing

**It invents detail.** Super-resolution reconstructs *plausible* pixels, it does not recover real ones. For UI and text this is reliable — Latin and Arabic both reconstruct cleanly in testing — but don't use it where pixel-exact fidelity matters: evidence, measurement, diffing, anything a person will make a factual claim from.

**WebP by default.** At q90 it measures 0.77% RMSE against lossless while being ~18× smaller. That difference is invisible, and it is what keeps a full-resolution 4096 px image around 0.20 MB instead of 3.58 MB — comfortably inside a tight size budget rather than well over it. Use `--png` if you want lossless.

**The real fix is upstream.** If your desktop renders at 2× (200% display scaling), applications draw at HiDPI and your screenshots contain *genuine* detail instead of reconstructed detail. Nothing downstream beats capturing the pixels for real. `quality` is for the screenshots you already took, and for when you don't want to run your desktop at 2×.

**Why `ort` and not a pure-Rust runtime.** `tract` would have made this dependency-free, and it benchmarked at **314 s/MP against ort's ~25** — eight minutes for one 1780×873 screenshot. Not worth the purity.

---

## License

MIT — see [LICENSE](LICENSE).

Model: [Real-ESRGAN](https://github.com/xinntao/Real-ESRGAN) `realesr-general-x4v3` by Xintao Wang et al., BSD-3-Clause.
