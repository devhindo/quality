# quality

Make screenshots crisp enough to post.

Linux screenshots look worse on social media than Mac ones for a physical reason: macOS captures a Retina display, so a 296×423 region becomes 592×846 actual pixels. On a 1× Linux display you get 296×423. When the browser renders that in a timeline it has to *upscale*, and browser upscaling is bilinear mush.

`quality` fixes it with super-resolution — a model that reconstructs detail rather than interpolating between the pixels you already have — then fits the result to your target platform's upload ceiling.

```
quality screenshot.png
```

## Install

```
cargo install --path .
```

The 4.9 MB model is embedded in the binary. No separate download, no Python, no ffmpeg.

## Usage

```
quality shot.png                      # -> shot-quality.webp, fitted for X
quality shot.png -t bluesky           # 2000px / 1 MB ceiling
quality shot.png --png                # lossless PNG instead
quality shot.png -i 40                # gentler ML effect
quality shot.png -o out.webp
```

| Flag | Default | What it does |
|------|---------|--------------|
| `-t, --target` | `x` | `x`, `mastodon`, `bluesky`, `instagram`, `none` |
| `-i, --intensity` | `60` | ML strength, 0–100. Lower is softer |
| `-s, --saturation` | `92` | Saturation percent |
| `-q, --quality` | `90` | WebP quality |
| `--png` | off | Lossless PNG output |
| `--tile` | `384` | Inference tile size; lower it if memory is tight |

## Platform ceilings

| Target | Max long edge | Max size |
|---|---|---|
| `x` / `twitter` | 4096 px | 5 MB |
| `mastodon` | 4096 px | 8 MB |
| `bluesky` | 2000 px | 1 MB |
| `instagram` | 1440 px | 8 MB |
| `none` | unlimited | unlimited |

The model always runs at 4×, then downsamples to the target. That beats upscaling directly to the target size, because downsampling also averages away model artifacts.

## How it works

1. **Super-resolution** — Real-ESRGAN `general-x4v3` at 4×. Large images are tiled with 24px overlap so peak memory stays bounded; the overlap is cropped from each tile, so seams carry no tile-edge artifacts.
2. **Blend back toward Lanczos** — the model raises local contrast about 29%, which blows highlights toward pure white and makes colours read as oversaturated. `--intensity` controls how much of the model's output survives; the default keeps 60%.
3. **Saturation trim** — a small correction for the same effect.
4. **Fit and encode** — downsample to the platform ceiling, encode WebP (or PNG).

## Notes

- Super-resolution *invents* plausible detail. For UI and text it's reliable — Latin and Arabic both reconstruct cleanly in testing — but it is a reconstruction, not recovery. Don't use it where pixel-exact fidelity matters, like evidence or measurement.
- WebP q90 measures 0.77% RMSE against lossless while being ~18× smaller, so the default costs you nothing visible.
- The real fix upstream is capturing at 2× in the first place: set GNOME display scaling to 200% and apps render HiDPI, giving genuine detail instead of reconstructed detail. Nothing downstream beats that.

## Performance

Roughly 25–30 s/megapixel on CPU.

| Input | Output | Time |
|---|---|---|
| 296×423 | 1184×1692 | ~3 s |
| 1780×873 | 4096×2009 | ~50 s |

## License

MIT
