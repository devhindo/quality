//! quality - make screenshots crisp enough to post.
//!
//! Pipeline: Real-ESRGAN 4x super-resolution, blended back toward a Lanczos
//! upscale to tame the model's contrast boost, then fitted to the upload
//! ceiling of whichever platform you're posting to.

use anyhow::{Context, Result, bail};
use clap::Parser;
use image::{DynamicImage, RgbImage, RgbaImage, imageops::FilterType};
use ort::{session::Session, value::Tensor};
use std::path::{Path, PathBuf};
use std::time::Instant;

const MODEL: &[u8] = include_bytes!("../models/general-x4v3.onnx");
const SCALE: u32 = 4;

/// Upload ceilings: (max long edge px, max file bytes).
fn preset(name: &str) -> Option<(u32, u64)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "x" | "twitter" => (4096, 5_000_000),
        "mastodon" => (4096, 8_000_000),
        "bluesky" => (2000, 1_000_000),
        "instagram" | "ig" => (1440, 8_000_000),
        "none" | "full" => (u32::MAX, u64::MAX),
        _ => return None,
    })
}

#[derive(Parser, Debug)]
#[command(
    name = "quality",
    version,
    about = "Make screenshots crisp enough to post"
)]
struct Args {
    /// Screenshot to upscale
    input: PathBuf,

    /// Output file (default: <input>-quality.webp)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Platform to fit: x, mastodon, bluesky, instagram, none
    #[arg(short, long, default_value = "x")]
    target: String,

    /// Strength of the ML effect, 0-100. Lower = softer, closer to plain Lanczos
    #[arg(short, long, default_value_t = 60)]
    intensity: u8,

    /// Saturation percent (100 = unchanged)
    #[arg(short, long, default_value_t = 92)]
    saturation: u16,

    /// Write lossless PNG instead of WebP
    #[arg(long)]
    png: bool,

    /// WebP quality, 0-100
    #[arg(short, long, default_value_t = 90.0)]
    quality: f32,

    /// Tile size for inference; lower it if you run out of memory
    #[arg(long, default_value_t = 384)]
    tile: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.intensity > 100 {
        bail!("--intensity must be 0-100, got {}", args.intensity);
    }
    let (max_dim, max_bytes) = preset(&args.target).with_context(|| {
        format!(
            "unknown --target {:?} (try: x, mastodon, bluesky, instagram, none)",
            args.target
        )
    })?;

    let src = image::open(&args.input)
        .with_context(|| format!("could not read {}", args.input.display()))?;
    let (sw, sh) = (src.width(), src.height());
    let rgb = src.to_rgb8();
    let alpha = has_alpha(&src).then(|| src.to_rgba8());

    eprintln!(
        "input   {sw}x{sh}  ({:.2} MP)",
        (sw as f64 * sh as f64) / 1e6
    );

    // 1. Super-resolve 4x. Tiling keeps peak memory bounded on large inputs.
    let t0 = Instant::now();
    let mut session = Session::builder()?
        .commit_from_memory(MODEL)
        .context("failed to load embedded super-resolution model")?;
    let sr = super_resolve(&mut session, &rgb, args.tile)?;
    eprintln!(
        "upscale {}x{}  in {:.1}s",
        sr.width(),
        sr.height(),
        t0.elapsed().as_secs_f64()
    );

    // 2. Blend back toward Lanczos. The model raises local contrast ~29%,
    //    which blows out whites; this dials that back without losing detail.
    let lanczos = image::imageops::resize(&rgb, sr.width(), sr.height(), FilterType::Lanczos3);
    let mut out = blend(&sr, &lanczos, args.intensity as f32 / 100.0);

    // 3. Saturation trim (the contrast boost also reads as extra colour).
    if args.saturation != 100 {
        saturate(&mut out, args.saturation as f32 / 100.0);
    }

    // 4. Fit the platform ceiling. Downsampling a 4x result beats upscaling
    //    straight to the target, so we always run the model at full 4x first.
    let out = fit(out, max_dim);
    let mut final_img = DynamicImage::ImageRgb8(out);
    if let Some(a) = alpha {
        final_img = reattach_alpha(final_img, &a);
    }

    // 5. Encode.
    let path = args
        .output
        .unwrap_or_else(|| default_output(&args.input, args.png));
    let bytes = encode(&final_img, &path, args.quality)?;
    std::fs::write(&path, &bytes).with_context(|| format!("could not write {}", path.display()))?;

    let over = bytes.len() as u64 > max_bytes;
    eprintln!(
        "output  {}x{}  {:.2} MB  -> {}{}",
        final_img.width(),
        final_img.height(),
        bytes.len() as f64 / 1e6,
        path.display(),
        if over {
            "  [WARNING: over platform size limit]"
        } else {
            ""
        }
    );
    if over {
        eprintln!(
            "        {} allows {:.1} MB; try --quality {} or --target bluesky",
            args.target,
            max_bytes as f64 / 1e6,
            (args.quality - 10.0).max(50.0)
        );
    }
    Ok(())
}

fn has_alpha(img: &DynamicImage) -> bool {
    use image::ColorType::*;
    matches!(img.color(), La8 | La16 | Rgba8 | Rgba16 | Rgba32F)
}

/// Run the model over the image, tiling with overlap so peak memory stays
/// bounded. The overlap is cropped off each result, so seams never carry
/// tile-edge artifacts.
fn super_resolve(session: &mut Session, img: &RgbImage, tile: u32) -> Result<RgbImage> {
    let (w, h) = img.dimensions();
    let mut out = RgbImage::new(w * SCALE, h * SCALE);

    if w * h <= tile * tile {
        return infer(session, img);
    }

    let pad = 24u32;
    for y0 in (0..h).step_by(tile as usize) {
        for x0 in (0..w).step_by(tile as usize) {
            let (x1, y1) = ((x0 + tile).min(w), (y0 + tile).min(h));
            let (ex0, ey0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
            let (ex1, ey1) = ((x1 + pad).min(w), (y1 + pad).min(h));

            let patch = image::imageops::crop_imm(img, ex0, ey0, ex1 - ex0, ey1 - ey0).to_image();
            let res = infer(session, &patch)?;

            // Copy only the un-padded core into place.
            let (cx, cy) = ((x0 - ex0) * SCALE, (y0 - ey0) * SCALE);
            for y in 0..(y1 - y0) * SCALE {
                for x in 0..(x1 - x0) * SCALE {
                    out.put_pixel(
                        x0 * SCALE + x,
                        y0 * SCALE + y,
                        *res.get_pixel(cx + x, cy + y),
                    );
                }
            }
        }
    }
    Ok(out)
}

fn infer(session: &mut Session, patch: &RgbImage) -> Result<RgbImage> {
    let (w, h) = patch.dimensions();
    let plane = (w * h) as usize;
    let mut data = vec![0f32; plane * 3];
    for (i, p) in patch.pixels().enumerate() {
        data[i] = p[0] as f32 / 255.0;
        data[plane + i] = p[1] as f32 / 255.0;
        data[plane * 2 + i] = p[2] as f32 / 255.0;
    }

    let name = session.inputs()[0].name().to_string();
    let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], data))?;
    let outputs = session.run(ort::inputs![name => input])?;
    let (shape, vals) = outputs[0].try_extract_tensor::<f32>()?;

    let (oh, ow) = (shape[2] as u32, shape[3] as u32);
    let oplane = (ow * oh) as usize;
    let mut img = RgbImage::new(ow, oh);
    for (i, px) in img.pixels_mut().enumerate() {
        px[0] = to_u8(vals[i]);
        px[1] = to_u8(vals[oplane + i]);
        px[2] = to_u8(vals[oplane * 2 + i]);
    }
    Ok(img)
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `k` is how much of the ML result to keep; the rest comes from Lanczos.
fn blend(sr: &RgbImage, lanczos: &RgbImage, k: f32) -> RgbImage {
    let mut out = sr.clone();
    for (o, l) in out.pixels_mut().zip(lanczos.pixels()) {
        for c in 0..3 {
            o[c] = (o[c] as f32 * k + l[c] as f32 * (1.0 - k))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn saturate(img: &mut RgbImage, amount: f32) {
    for p in img.pixels_mut() {
        let lum = 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
        for c in 0..3 {
            p[c] = (lum + (p[c] as f32 - lum) * amount)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

fn fit(img: RgbImage, max_dim: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    if w.max(h) <= max_dim {
        return img;
    }
    let scale = max_dim as f64 / w.max(h) as f64;
    let (nw, nh) = (
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    );
    image::imageops::resize(&img, nw, nh, FilterType::Lanczos3)
}

fn reattach_alpha(img: DynamicImage, src_rgba: &RgbaImage) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let a = image::imageops::resize(src_rgba, w, h, FilterType::Lanczos3);
    let rgb = img.to_rgb8();
    let mut out = RgbaImage::new(w, h);
    for (o, (p, s)) in out.pixels_mut().zip(rgb.pixels().zip(a.pixels())) {
        *o = image::Rgba([p[0], p[1], p[2], s[3]]);
    }
    DynamicImage::ImageRgba8(out)
}

fn default_output(input: &Path, png: bool) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".into());
    let ext = if png { "png" } else { "webp" };
    input.with_file_name(format!("{stem}-quality.{ext}"))
}

fn encode(img: &DynamicImage, path: &Path, quality: f32) -> Result<Vec<u8>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("webp")
        .to_ascii_lowercase();
    match ext.as_str() {
        "webp" => {
            let mem = match img {
                DynamicImage::ImageRgba8(a) => {
                    webp::Encoder::from_rgba(a.as_raw(), a.width(), a.height()).encode(quality)
                }
                other => {
                    let r = other.to_rgb8();
                    webp::Encoder::from_rgb(r.as_raw(), r.width(), r.height()).encode(quality)
                }
            };
            Ok(mem.to_vec())
        }
        _ => {
            let mut buf = std::io::Cursor::new(Vec::new());
            let fmt = image::ImageFormat::from_extension(&ext)
                .with_context(|| format!("unsupported output format {ext:?}"))?;
            img.write_to(&mut buf, fmt)?;
            Ok(buf.into_inner())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_case_insensitive_and_aliased() {
        assert_eq!(preset("x"), preset("X"));
        assert_eq!(preset("x"), preset("twitter"));
        assert_eq!(preset("instagram"), preset("ig"));
        assert_eq!(preset("none"), preset("full"));
        assert!(preset("tumblr").is_none());
    }

    #[test]
    fn bluesky_is_the_tightest_ceiling() {
        // Bluesky's 1 MB cap is what forces the WebP default, so if it ever
        // stops being the strictest, that encoding tradeoff needs revisiting.
        let (dim, bytes) = preset("bluesky").unwrap();
        assert_eq!((dim, bytes), (2000, 1_000_000));
        for other in ["x", "mastodon", "instagram"] {
            assert!(preset(other).unwrap().1 > bytes);
        }
    }

    #[test]
    fn fit_preserves_aspect_ratio_and_caps_long_edge() {
        let wide = RgbImage::new(4000, 1000);
        assert_eq!(fit(wide, 2000).dimensions(), (2000, 500));

        let tall = RgbImage::new(1000, 4000);
        assert_eq!(fit(tall, 2000).dimensions(), (500, 2000));
    }

    #[test]
    fn fit_never_upscales() {
        let small = RgbImage::new(100, 50);
        assert_eq!(fit(small, 4096).dimensions(), (100, 50));
    }

    #[test]
    fn fit_never_collapses_a_dimension_to_zero() {
        // An extreme aspect ratio rounds the short edge toward 0; clamping to
        // 1 is what stops image::resize panicking on a zero-sized buffer.
        let sliver = RgbImage::new(10000, 3);
        let out = fit(sliver, 100);
        assert!(out.height() >= 1, "height collapsed to {}", out.height());
    }

    #[test]
    fn default_output_swaps_extension_and_keeps_directory() {
        assert_eq!(
            default_output(Path::new("/tmp/shot.png"), false),
            PathBuf::from("/tmp/shot-quality.webp")
        );
        assert_eq!(
            default_output(Path::new("/tmp/shot.jpeg"), true),
            PathBuf::from("/tmp/shot-quality.png")
        );
        // Names containing dots must not be truncated at the first one.
        assert_eq!(
            default_output(Path::new("v1.2.final.png"), false),
            PathBuf::from("v1.2.final-quality.webp")
        );
    }

    #[test]
    fn saturate_at_100_percent_is_a_no_op() {
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgb([10, 200, 90]));
        img.put_pixel(1, 0, image::Rgb([255, 0, 0]));
        let before = img.clone();
        saturate(&mut img, 1.0);
        assert_eq!(img, before);
    }

    #[test]
    fn saturate_at_zero_produces_grey() {
        let mut img = RgbImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgb([10, 200, 90]));
        saturate(&mut img, 0.0);
        let p = img.get_pixel(0, 0);
        assert_eq!(p[0], p[1]);
        assert_eq!(p[1], p[2]);
    }

    #[test]
    fn blend_endpoints_select_each_source() {
        let mut sr = RgbImage::new(1, 1);
        sr.put_pixel(0, 0, image::Rgb([200, 200, 200]));
        let mut lz = RgbImage::new(1, 1);
        lz.put_pixel(0, 0, image::Rgb([100, 100, 100]));

        assert_eq!(*blend(&sr, &lz, 1.0).get_pixel(0, 0), image::Rgb([200; 3]));
        assert_eq!(*blend(&sr, &lz, 0.0).get_pixel(0, 0), image::Rgb([100; 3]));
        assert_eq!(*blend(&sr, &lz, 0.5).get_pixel(0, 0), image::Rgb([150; 3]));
    }

    #[test]
    fn to_u8_clamps_out_of_range_model_output() {
        // Model output is unbounded, so values outside 0..1 are normal and
        // must not wrap around when cast to u8.
        assert_eq!(to_u8(-5.0), 0);
        assert_eq!(to_u8(0.0), 0);
        assert_eq!(to_u8(1.0), 255);
        assert_eq!(to_u8(9.9), 255);
        assert_eq!(to_u8(f32::NAN), 0);
    }
}
