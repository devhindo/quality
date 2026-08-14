<div align="center">

# quality

**Make screenshots crisp enough to post.**

[![release](https://github.com/devhindo/quality/actions/workflows/release.yml/badge.svg)](https://github.com/devhindo/quality/actions/workflows/release.yml)
[![latest](https://img.shields.io/github/v/release/devhindo/quality?label=latest)](https://github.com/devhindo/quality/releases/latest)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

```
quality screenshot.png
```

</div>

---

## Why your Linux screenshots look worse than everyone's Mac ones

It isn't your compression settings, and it isn't the screenshot tool. It's pixel count.

macOS captures a Retina display, so a 296×423 region on screen becomes **592×846 actual pixels** in the file. On a 1× Linux display, the same region is **296×423**. When Twitter renders that in a timeline it has to *upscale* it — and browser upscaling is bilinear, which is mush.

So the Mac user uploads an image with 4× the information and the browser downsamples it, which looks sharp. You upload the smaller one and the browser stretches it, which looks soft. Same screen, same app, same content.

**This cannot be fixed by sharpening.** Lanczos, unsharp mask, and every filter of that family only redistribute information that is already in the file — 296×423 is ~125,000 pixels of information no matter what you do to it.

`quality` runs a super-resolution model instead, which *reconstructs* detail rather than interpolating between the pixels you have, then fits the result to your target platform's upload ceiling.

---

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/install.sh | sh
```

Installs to `~/.local/bin` and never asks for `sudo`. For a system-wide install:

```sh
curl -fsSL https://raw.githubusercontent.com/devhindo/quality/main/install.sh | sudo QUALITY_INSTALL_DIR=/usr/local/bin sh
```

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

### Platforms

| OS | x86_64 | arm64 |
|---|---|---|
| Linux (glibc 2.38+) | ✅ | ✅ |
| macOS | ⚠️ source only | ✅ Apple Silicon |
| Windows | ✅ | — |

Two limits, both inherited from the prebuilt ONNX Runtime this links against rather than chosen:

- **glibc 2.38+ on Linux** — Ubuntu 23.10+, Debian 13+, Fedora 39+. The runtime is built against 2.38, so older systems cannot link it. Ubuntu 22.04 is still LTS and is *not* covered; those users need to build from source against a locally built runtime.
- **No Intel Mac binary.** `ort` publishes no prebuilt runtime for `x86_64-apple-darwin` — *"no prebuilt binaries available for target x86_64-apple-darwin"* — so there is nothing to ship. Apple Silicon is fully supported; Intel Mac users must build ONNX Runtime themselves.

One self-contained binary. The 4.9 MB model is compiled into it — no Python, no ffmpeg, no ONNX Runtime install, no model download on first run.

---

## Usage

```sh
quality shot.png                # -> shot-quality.webp, fitted for X
quality shot.png -t bluesky     # 2000 px / 1 MB ceiling
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
| `-t, --target` | `x` | Platform to fit: `x`, `mastodon`, `bluesky`, `instagram`, `none` |
| `-i, --intensity` | `60` | ML strength, 0–100. Lower is softer and closer to plain Lanczos |
| `-s, --saturation` | `92` | Saturation percent; `100` leaves colour untouched |
| `-q, --quality` | `90` | WebP quality |
| `--png` | off | Write lossless PNG instead of WebP |
| `--tile` | `384` | Inference tile size. Lower it if you run out of memory |
| `-o, --output` | `<input>-quality.webp` | Output path; extension picks the format |

### Platform ceilings

| Target | Max long edge | Max file size |
|---|---|---|
| `x` / `twitter` | 4096 px | 5 MB |
| `mastodon` | 4096 px | 8 MB |
| `bluesky` | 2000 px | 1 MB |
| `instagram` / `ig` | 1440 px | 8 MB |
| `none` / `full` | unlimited | unlimited |

If the encoded file still lands over the size ceiling, `quality` says so and suggests a lower `--quality` rather than silently shipping something the platform will re-compress.

---

## How it works

**1. Super-resolution at 4×.** Real-ESRGAN `general-x4v3` (SRVGGNetCompact). Images above one tile are processed in 384 px tiles with 24 px of overlap; the overlap is cropped off each result, so no seam ever carries a tile-edge artifact. This bounds peak memory to roughly 1.4 GB — full-frame inference on a 1.55 MP screenshot will OOM a 16 GB machine that's already busy.

**2. Blend back toward Lanczos.** Straight model output looks *wrong* in a specific way: whites read as whiter than they are, and everything looks over-saturated. Measuring it showed mean RGB and mean saturation were **identical** to a Lanczos upscale — the drift was **~29% more local contrast** (std dev 36 vs 28). So the correction is a blend against Lanczos, not a saturation fix. `--intensity` is that blend; the default keeps 60% of the model.

**3. Saturation trim.** A small extra correction for the same effect, at 92%.

**4. Fit and encode.** Downsample to the platform ceiling, then WebP (or PNG). The model always runs at full 4× first even when the target is smaller — downsampling a 4× result beats upscaling straight to the target, because it also averages away model artifacts.

### Performance

Roughly 25–30 s/megapixel on CPU. There is no GPU path.

| Input | Output | Time |
|---|---|---|
| 296×423 | 1184×1692 | ~3 s |
| 1780×873 | 4096×2009 | ~50 s |

---

## Things worth knowing

**It invents detail.** Super-resolution reconstructs *plausible* pixels, it does not recover real ones. For UI and text this is reliable — Latin and Arabic both reconstruct cleanly in testing — but don't use it where pixel-exact fidelity matters: evidence, measurement, diffing, anything a person will make a factual claim from.

**WebP by default.** At q90 it measures 0.77% RMSE against lossless while being ~18× smaller. That difference is invisible, and it's what keeps a full-resolution 4096 px image at 0.20 MB instead of 3.58 MB — which is the difference between clearing Bluesky's 1 MB limit and not. Use `--png` if you want lossless.

**The real fix is upstream.** If you set GNOME display scaling to 200%, apps render HiDPI and your screenshots contain *genuine* detail instead of reconstructed detail. Nothing downstream beats capturing the pixels for real. `quality` is for the screenshots you already took, and for when you don't want to run your desktop at 2×.

**Why `ort` and not a pure-Rust runtime.** `tract` would have made this dependency-free, and it benchmarked at **314 s/MP against ort's ~25** — eight minutes for one 1780×873 screenshot. Not worth the purity.

---

## License

MIT — see [LICENSE](LICENSE).

Model: [Real-ESRGAN](https://github.com/xinntao/Real-ESRGAN) `realesr-general-x4v3` by Xintao Wang et al., BSD-3-Clause.
