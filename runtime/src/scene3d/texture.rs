//! Validated immutable RGBA8 images for 3D objects and 2D pictures, with their mip chain. Decoding image formats
//! lives outside this boundary; this owns size limits and filtering.

use peniko::Blob;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

pub const MAX_TEXTURE_SIDE: u32 = 2048;

pub struct TextureData {
    id: u64,
    width: u32,
    height: u32,
    /// Level 0 first, each level half the previous (rounded down, at least 1), down to 1x1.
    /// Blobs so a 2D picture draws level 0 itself rather than a copy.
    pub(super) mips: Box<[Blob<u8>]>,
}

/// Size and identity only: pixel dumps in a failed assertion are megabytes of noise.
impl std::fmt::Debug for TextureData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextureData")
            .field("id", &self.id)
            .field("size", &(self.width, self.height))
            .finish()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextureError {
    #[error("texture must be 1..2048 pixels on each side")]
    InvalidSize,
    #[error("texture pixel data does not match width x height x 4")]
    InvalidData,
}

/// Bytes of a `width` x `height` texture with its full mip chain.
pub fn payload_bytes(width: u32, height: u32) -> usize {
    let (mut w, mut h, mut total) = (width as usize, height as usize, 0);
    loop {
        total += w * h * 4;
        if w == 1 && h == 1 {
            return total;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
}

impl TextureData {
    /// `rgba` is row-major, top row first, sRGB-encoded colour with straight alpha.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Arc<Self>, TextureError> {
        if !(1..=MAX_TEXTURE_SIDE).contains(&width) || !(1..=MAX_TEXTURE_SIDE).contains(&height) {
            return Err(TextureError::InvalidSize);
        }
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(TextureError::InvalidData);
        }
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let mut mips = vec![Blob::from(rgba)];
        let (mut w, mut h) = (width as usize, height as usize);
        while w > 1 || h > 1 {
            let next = Blob::from(downsample(mips.last().unwrap().data(), w, h));
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            mips.push(next);
        }
        Ok(Arc::new(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            mips: mips.into(),
        }))
    }

    pub fn resource_id(&self) -> u64 {
        self.id
    }
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn pixels(&self) -> &[u8] {
        self.mips[0].data()
    }
    pub(crate) fn level0(&self) -> &Blob<u8> {
        &self.mips[0]
    }
    pub fn payload_bytes(&self) -> usize {
        self.mips.iter().map(|m| m.len()).sum()
    }
}

fn to_linear(v: u8) -> f32 {
    let c = v as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> u8 {
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Box filter to the next level, averaging colour in linear light so edges do not darken.
/// Both conversions are tables: a powf per channel per pixel dominated large loads.
fn downsample(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    const STEPS: usize = 4096;
    let linear: Vec<f32> = (0..=255).map(to_linear).collect();
    let encode: Vec<u8> = (0..STEPS).map(|i| to_srgb(i as f32 / (STEPS - 1) as f32)).collect();
    let to_srgb = |c: f32| encode[(c * (STEPS - 1) as f32 + 0.5) as usize];
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0f32; 4];
            let mut n = 0.0;
            for sy in (y * 2)..(y * 2 + 2).min(h) {
                for sx in (x * 2)..(x * 2 + 2).min(w) {
                    let p = &src[(sy * w + sx) * 4..][..4];
                    for c in 0..3 {
                        sum[c] += linear[p[c] as usize];
                    }
                    sum[3] += p[3] as f32;
                    n += 1.0;
                }
            }
            let o = &mut out[(y * nw + x) * 4..][..4];
            for c in 0..3 {
                o[c] = to_srgb(sum[c] / n);
            }
            o[3] = (sum[3] / n).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests;
