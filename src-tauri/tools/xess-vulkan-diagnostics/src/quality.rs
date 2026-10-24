use crate::{api::Size, Result};
use serde_json::{json, Value};
use std::{fs::File, io::BufWriter, path::Path};

/// Four 1:1 crops: wires, checker, rings and the moving foreground boundary.
pub fn crops(pixels: &[f32], size: Size) -> (Size, Vec<f32>) {
    let w = 256.min(size.x / 3);
    let h = 144.min(size.y / 3);
    let out = Size { x: w * 2, y: h * 2 };
    let mut crop = vec![0.0; out.x as usize * out.y as usize * 4];
    for (n, (cx, cy)) in [(0.17, 0.24), (0.50, 0.24), (0.82, 0.24), (0.65, 0.62)]
        .into_iter()
        .enumerate()
    {
        let sx = ((size.x as f32 * cx) as u32)
            .saturating_sub(w / 2)
            .min(size.x - w);
        let sy = ((size.y as f32 * cy) as u32)
            .saturating_sub(h / 2)
            .min(size.y - h);
        for y in 0..h {
            for x in 0..w {
                let src = (((sy + y) * size.x + sx + x) * 4) as usize;
                let dst = (((n as u32 / 2 * h + y) * out.x + n as u32 % 2 * w + x) * 4) as usize;
                crop[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
            }
        }
    }
    (out, crop)
}

pub fn png(path: &Path, size: Size, pixels: &[f32]) -> Result<()> {
    if pixels.iter().any(|p| !p.is_finite()) {
        return Err("nonfinite output pixels".into());
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), size.x, size.y);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let rgb: Vec<_> = pixels
        .chunks_exact(4)
        .flat_map(|p| {
            p[..3].iter().map(|&v| {
                let v = v.clamp(0.0, 1.0);
                let v = if v <= 0.0031308 {
                    12.92 * v
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                };
                (v * 255.0).round() as u8
            })
        })
        .collect();
    encoder.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
pub fn bilinear(input: &[f32], size: Size, output: Size, jitter: [f32; 2]) -> Vec<f32> {
    let mut pixels = vec![0.0; output.x as usize * output.y as usize * 4];
    for y in 0..output.y {
        for x in 0..output.x {
            let qx = (x as f32 + 0.5) * size.x as f32 / output.x as f32 + jitter[0] - 0.5;
            let qy = (y as f32 + 0.5) * size.y as f32 / output.y as f32 + jitter[1] - 0.5;
            let ix = qx.floor() as i32;
            let iy = qy.floor() as i32;
            let fx = qx - qx.floor();
            let fy = qy - qy.floor();
            let dst = ((y * output.x + x) * 4) as usize;
            for (dx, dy, w) in [
                (0, 0, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let sx = (ix + dx).clamp(0, size.x as i32 - 1) as u32;
                let sy = (iy + dy).clamp(0, size.y as i32 - 1) as u32;
                let src = ((sy * size.x + sx) * 4) as usize;
                for c in 0..4 {
                    pixels[dst + c] += input[src + c] * w;
                }
            }
        }
    }
    pixels
}
pub fn metric(pixels: &[f32], reference: &[f32], size: Size) -> Value {
    let mut regions = serde_json::Map::new();
    for (name, rect) in [
        ("whole", [0.0, 0.0, 1.0, 1.0]),
        ("wires", [0.03, 0.04, 0.30, 0.43]),
        ("checker", [0.36, 0.04, 0.63, 0.43]),
        ("rings", [0.69, 0.04, 0.97, 0.43]),
        ("foreground", [0.35, 0.49, 0.80, 0.93]),
    ] {
        let mut squared = 0.0;
        let mut absolute = 0.0;
        let mut count = 0;
        // Exclude the outer eight pixels, where reprojection leaves the image.
        for y in ((rect[1] * size.y as f32) as u32).max(8)
            ..((rect[3] * size.y as f32) as u32).min(size.y - 8)
        {
            for x in ((rect[0] * size.x as f32) as u32).max(8)
                ..((rect[2] * size.x as f32) as u32).min(size.x - 8)
            {
                let i = ((y * size.x + x) * 4) as usize;
                for c in 0..3 {
                    let d = (pixels[i + c] - reference[i + c]) as f64;
                    squared += d * d;
                    absolute += d.abs();
                    count += 1;
                }
            }
        }
        let mse = squared / count as f64;
        regions.insert(name.into(),json!({"linear_rgb_rmse":mse.sqrt(),"linear_rgb_mae":absolute/count as f64,"psnr_db":if mse>0.0 {Some(-10.0*mse.log10())}else{None}}));
    }
    json!(regions)
}
pub fn distribution(samples: &[f64]) -> Value {
    if samples.is_empty() {
        return Value::Null;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({"samples":sorted.len(),"mean":sorted.iter().sum::<f64>()/sorted.len() as f64,
        "p50":sorted[(sorted.len()-1)/2],"p95":sorted[((sorted.len() as f64*0.95).ceil() as usize-1).min(sorted.len()-1)],"min":sorted[0],"max":sorted[sorted.len()-1]})
}
