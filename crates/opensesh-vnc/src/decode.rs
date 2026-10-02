//! The encodings this client asks for (RFC 6143 §7.7 and the community `rfbproto`): Raw,
//! CopyRect, Hextile, ZRLE and Tight (its zlib streams, palette and gradient filters, and JPEG),
//! drawn into the [`Canvas`]; and the pseudo-encodings for the cursor and the desktop's size.
//!
//! Pixels arrive in the format the session asks for: 32 bits, little-endian, red in the low byte
//! (`[r, g, b, x]` in memory). ZRLE's CPIXEL and Tight's TPIXEL are then its first three bytes.

use flate2::{Decompress, FlushDecompress, Status};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::VncError;
use crate::canvas::{Area, Canvas};

/// Raw.
pub const RAW: i32 = 0;
/// CopyRect.
pub const COPY_RECT: i32 = 1;
/// Hextile.
pub const HEXTILE: i32 = 5;
/// Tight.
pub const TIGHT: i32 = 7;
/// ZRLE.
pub const ZRLE: i32 = 16;
/// The cursor's shape.
pub const CURSOR: i32 = -239;
/// The desktop's new size.
pub const DESKTOP_SIZE: i32 = -223;
/// No more rectangles in this update (Tight's servers use it).
pub const LAST_RECT: i32 = -224;
/// The desktop's new size, with screens and the reason.
pub const EXTENDED_DESKTOP_SIZE: i32 = -308;
/// Tight's JPEG quality, 0 to 9: `QUALITY_0 + level`.
pub const QUALITY_0: i32 = -32;
/// Tight's and ZRLE's zlib level, 0 to 9: `COMPRESS_0 + level`.
pub const COMPRESS_0: i32 = -256;

/// The largest compressed block a rectangle may carry (ZRLE, Tight).
const MAX_COMPRESSED: usize = 64 << 20;

/// A cursor picture: RGBA (transparent where the mask is off), with its hot spot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
    /// Hot spot X.
    pub hot_x: u16,
    /// Hot spot Y.
    pub hot_y: u16,
    /// RGBA, not premultiplied.
    pub rgba: Vec<u8>,
}

async fn read_bytes<R: AsyncRead + Unpin>(reader: &mut R, len: usize) -> Result<Vec<u8>, VncError> {
    let mut buffer = vec![0; len];
    reader.read_exact(&mut buffer).await?;
    Ok(buffer)
}

/// One pixel in the session's format, as RGB.
async fn read_pixel<R: AsyncRead + Unpin>(reader: &mut R) -> Result<[u8; 3], VncError> {
    let mut pixel = [0; 4];
    reader.read_exact(&mut pixel).await?;
    Ok([pixel[0], pixel[1], pixel[2]])
}

/// A compact length (Tight): 1 to 3 bytes, 7 bits each.
async fn read_compact<R: AsyncRead + Unpin>(reader: &mut R) -> Result<usize, VncError> {
    let first = reader.read_u8().await?;
    let mut length = usize::from(first & 0x7F);
    if first & 0x80 != 0 {
        let second = reader.read_u8().await?;
        length |= usize::from(second & 0x7F) << 7;
        if second & 0x80 != 0 {
            length |= usize::from(reader.read_u8().await?) << 14;
        }
    }
    Ok(length)
}

/// Inflates `input` on a persistent zlib stream until `expected` bytes came out (or, with `None`,
/// until it gives no more), at most `limit` bytes.
fn inflate(
    stream: &mut Decompress,
    input: &[u8],
    expected: Option<usize>,
    limit: usize,
) -> Result<Vec<u8>, VncError> {
    let mut output = Vec::with_capacity(expected.unwrap_or(input.len() * 4).min(limit).max(64));
    let mut consumed = 0;
    loop {
        if expected.is_some_and(|expected| output.len() >= expected) {
            break;
        }
        if output.len() == output.capacity() {
            if output.len() > limit {
                return Err(VncError::Protocol(
                    "a compressed rectangle is too big".into(),
                ));
            }
            output.reserve(output.len().max(4096).min(limit + 1 - output.len()));
        }
        let before_in = stream.total_in();
        let before_out = stream.total_out();
        let status = stream
            .decompress_vec(&input[consumed..], &mut output, FlushDecompress::Sync)
            .map_err(|error| VncError::Protocol(format!("zlib: {error}")))?;
        let read = usize::try_from(stream.total_in() - before_in).unwrap_or(usize::MAX);
        let written = stream.total_out() - before_out;
        consumed = consumed.saturating_add(read).min(input.len());
        // With room left, no progress means the input is used up.
        if status == Status::StreamEnd || (read == 0 && written == 0) {
            break;
        }
    }
    match expected {
        Some(expected) if output.len() < expected => Err(VncError::Protocol(
            "a compressed rectangle ended early".into(),
        )),
        Some(expected) => {
            output.truncate(expected);
            Ok(output)
        }
        None if output.len() > limit => Err(VncError::Protocol(
            "a compressed rectangle is too big".into(),
        )),
        None => Ok(output),
    }
}

/// The decoders' state: ZRLE's zlib stream and Tight's four, kept for the whole connection.
pub struct Decoders {
    zrle: Decompress,
    tight: [Decompress; 4],
}

impl std::fmt::Debug for Decoders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoders").finish_non_exhaustive()
    }
}

impl Default for Decoders {
    fn default() -> Self {
        Self {
            zrle: Decompress::new(true),
            tight: [
                Decompress::new(true),
                Decompress::new(true),
                Decompress::new(true),
                Decompress::new(true),
            ],
        }
    }
}

impl Decoders {
    /// Raw: the pixels, row after row.
    ///
    /// # Errors
    ///
    /// On a read error.
    pub async fn raw<R: AsyncRead + Unpin>(
        reader: &mut R,
        canvas: &mut Canvas,
        area: Area,
    ) -> Result<(), VncError> {
        let pixels = read_bytes(reader, area.pixels() * 4).await?;
        canvas.put_rgbx(area, &pixels);
        Ok(())
    }

    /// CopyRect: where to copy from.
    ///
    /// # Errors
    ///
    /// On a read error, or a source outside the desktop.
    pub async fn copy_rect<R: AsyncRead + Unpin>(
        reader: &mut R,
        canvas: &mut Canvas,
        area: Area,
    ) -> Result<(), VncError> {
        let x = reader.read_u16().await?;
        let y = reader.read_u16().await?;
        canvas.check(Area { x, y, ..area })?;
        canvas.copy((x, y), area);
        Ok(())
    }

    /// Hextile: tiles of 16 by 16, each raw, or a background with rectangles on it.
    ///
    /// # Errors
    ///
    /// On a read error, or a subrectangle outside its tile.
    pub async fn hextile<R: AsyncRead + Unpin>(
        reader: &mut R,
        canvas: &mut Canvas,
        area: Area,
    ) -> Result<(), VncError> {
        const RAW_TILE: u8 = 1;
        const BACKGROUND: u8 = 2;
        const FOREGROUND: u8 = 4;
        const SUBRECTS: u8 = 8;
        const COLOURED: u8 = 16;
        let mut background = [0; 3];
        let mut foreground = [0; 3];
        for top in (0..area.height).step_by(16) {
            for left in (0..area.width).step_by(16) {
                let tile = Area {
                    x: area.x + left,
                    y: area.y + top,
                    width: (area.width - left).min(16),
                    height: (area.height - top).min(16),
                };
                let mask = reader.read_u8().await?;
                if mask & RAW_TILE != 0 {
                    Self::raw(reader, canvas, tile).await?;
                    continue;
                }
                if mask & BACKGROUND != 0 {
                    background = read_pixel(reader).await?;
                }
                canvas.fill(tile, background);
                if mask & FOREGROUND != 0 {
                    foreground = read_pixel(reader).await?;
                }
                if mask & SUBRECTS == 0 {
                    continue;
                }
                let count = reader.read_u8().await?;
                for _ in 0..count {
                    let colour = if mask & COLOURED != 0 {
                        read_pixel(reader).await?
                    } else {
                        foreground
                    };
                    let xy = reader.read_u8().await?;
                    let wh = reader.read_u8().await?;
                    let sub = Area {
                        x: u16::from(xy >> 4),
                        y: u16::from(xy & 0x0F),
                        width: u16::from(wh >> 4) + 1,
                        height: u16::from(wh & 0x0F) + 1,
                    };
                    if sub.x + sub.width > tile.width || sub.y + sub.height > tile.height {
                        return Err(VncError::Protocol(
                            "a Hextile subrectangle outside its tile".into(),
                        ));
                    }
                    canvas.fill(
                        Area {
                            x: tile.x + sub.x,
                            y: tile.y + sub.y,
                            ..sub
                        },
                        colour,
                    );
                }
            }
        }
        Ok(())
    }

    /// ZRLE: a zlib block of 64 by 64 tiles, each raw, one colour, a packed palette or runs.
    ///
    /// # Errors
    ///
    /// On a read error, bad zlib data, or tiles that don't fit.
    pub async fn zrle<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
        canvas: &mut Canvas,
        area: Area,
    ) -> Result<(), VncError> {
        let length = usize::try_from(reader.read_u32().await?).unwrap_or(usize::MAX);
        if length > MAX_COMPRESSED {
            return Err(VncError::Protocol("a ZRLE rectangle is too big".into()));
        }
        let compressed = read_bytes(reader, length).await?;
        let tiles = usize::from(area.width).div_ceil(64) * usize::from(area.height).div_ceil(64);
        // Raw tiles are the largest: three bytes a pixel and one a tile.
        let limit = area.pixels() * 3 + tiles;
        let data = inflate(&mut self.zrle, &compressed, None, limit)?;
        let mut input = Bytes::new(&data);
        for top in (0..area.height).step_by(64) {
            for left in (0..area.width).step_by(64) {
                let tile = Area {
                    x: area.x + left,
                    y: area.y + top,
                    width: (area.width - left).min(64),
                    height: (area.height - top).min(64),
                };
                zrle_tile(&mut input, canvas, tile)?;
            }
        }
        Ok(())
    }

    /// Tight: a fill, a JPEG picture, or zlib data with a copy, palette or gradient filter.
    ///
    /// # Errors
    ///
    /// On a read error, bad zlib or JPEG data, or a type this client didn't ask for.
    pub async fn tight<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
        canvas: &mut Canvas,
        area: Area,
    ) -> Result<(), VncError> {
        let control = reader.read_u8().await?;
        for (bit, stream) in self.tight.iter_mut().enumerate() {
            if control & (1 << bit) != 0 {
                stream.reset(true);
            }
        }
        match control >> 4 {
            0x8 => {
                let colour = read_tpixel(reader).await?;
                canvas.fill(area, colour);
                Ok(())
            }
            0x9 => {
                let length = read_compact(reader).await?;
                let data = read_bytes(reader, length).await?;
                let rgba = jpeg(&data, area)?;
                canvas.put_rgbx(area, &rgba);
                Ok(())
            }
            kind if kind & 0x8 == 0 => {
                let stream = usize::from(kind & 0x3);
                let filter = if kind & 0x4 != 0 {
                    reader.read_u8().await?
                } else {
                    0
                };
                let pixels = area.pixels();
                match filter {
                    // Copy: three bytes a pixel.
                    0 => {
                        let data = self.tight_data(reader, stream, pixels * 3).await?;
                        canvas.put_rgb(area, &data);
                    }
                    // Palette: 2 to 256 colours, then one bit (two colours) or one byte a pixel.
                    1 => {
                        let colours = usize::from(reader.read_u8().await?) + 1;
                        let palette = read_bytes(reader, colours * 3).await?;
                        let row = usize::from(area.width);
                        let length = if colours == 2 {
                            row.div_ceil(8) * usize::from(area.height)
                        } else {
                            pixels
                        };
                        let data = self.tight_data(reader, stream, length).await?;
                        let mut rgb = Vec::with_capacity(pixels * 3);
                        for y in 0..usize::from(area.height) {
                            for x in 0..row {
                                let index = if colours == 2 {
                                    let byte = data[y * row.div_ceil(8) + x / 8];
                                    usize::from((byte >> (7 - (x % 8))) & 1)
                                } else {
                                    usize::from(data[y * row + x])
                                };
                                let colour =
                                    palette.get(index * 3..index * 3 + 3).ok_or_else(|| {
                                        VncError::Protocol(
                                            "a Tight palette index out of range".into(),
                                        )
                                    })?;
                                rgb.extend_from_slice(colour);
                            }
                        }
                        canvas.put_rgb(area, &rgb);
                    }
                    // Gradient: each pixel as the difference from what its neighbours predict.
                    2 => {
                        let data = self.tight_data(reader, stream, pixels * 3).await?;
                        canvas.put_rgb(area, &gradient(&data, usize::from(area.width)));
                    }
                    other => {
                        return Err(VncError::Protocol(format!("unknown Tight filter {other}")));
                    }
                }
                Ok(())
            }
            other => Err(VncError::Protocol(format!(
                "a Tight rectangle of type {other:#x}, which wasn't asked for"
            ))),
        }
    }

    /// Tight's data: in clear below 12 bytes, else a compact length and zlib data.
    async fn tight_data<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
        stream: usize,
        length: usize,
    ) -> Result<Vec<u8>, VncError> {
        if length < 12 {
            return read_bytes(reader, length).await;
        }
        let compressed_length = read_compact(reader).await?;
        let compressed = read_bytes(reader, compressed_length).await?;
        inflate(&mut self.tight[stream], &compressed, Some(length), length)
    }

    /// The cursor pseudo-encoding: the pixels, then a bit mask.
    ///
    /// # Errors
    ///
    /// On a read error.
    pub async fn cursor<R: AsyncRead + Unpin>(
        reader: &mut R,
        area: Area,
    ) -> Result<Cursor, VncError> {
        let pixels = read_bytes(reader, area.pixels() * 4).await?;
        let row = usize::from(area.width).div_ceil(8);
        let mask = read_bytes(reader, row * usize::from(area.height)).await?;
        let mut rgba = Vec::with_capacity(area.pixels() * 4);
        for y in 0..usize::from(area.height) {
            for x in 0..usize::from(area.width) {
                let at = (y * usize::from(area.width) + x) * 4;
                let visible = mask[y * row + x / 8] & (0x80 >> (x % 8)) != 0;
                rgba.extend_from_slice(&[
                    pixels[at],
                    pixels[at + 1],
                    pixels[at + 2],
                    if visible { 0xFF } else { 0 },
                ]);
            }
        }
        Ok(Cursor {
            width: area.width,
            height: area.height,
            hot_x: area.x,
            hot_y: area.y,
            rgba,
        })
    }
}

/// A TPIXEL: three bytes, red first.
async fn read_tpixel<R: AsyncRead + Unpin>(reader: &mut R) -> Result<[u8; 3], VncError> {
    let mut pixel = [0; 3];
    reader.read_exact(&mut pixel).await?;
    Ok(pixel)
}

/// Tight's gradient filter undone: each component plus the prediction from the left, above and
/// above-left neighbours, clamped to 0..=255.
fn gradient(data: &[u8], width: usize) -> Vec<u8> {
    let mut out = vec![0_u8; data.len()];
    for (index, value) in data.iter().enumerate() {
        let pixel = index / 3;
        let (x, y) = (pixel % width, pixel / width);
        let component = index % 3;
        let at = |x: usize, y: usize| i32::from(out[(y * width + x) * 3 + component]);
        let left = if x > 0 { at(x - 1, y) } else { 0 };
        let above = if y > 0 { at(x, y - 1) } else { 0 };
        let corner = if x > 0 && y > 0 { at(x - 1, y - 1) } else { 0 };
        let predicted = u8::try_from((left + above - corner).clamp(0, 255)).unwrap_or(0);
        out[index] = value.wrapping_add(predicted);
    }
    out
}

/// A JPEG picture of exactly `area`'s size, as RGBA.
fn jpeg(data: &[u8], area: Area) -> Result<Vec<u8>, VncError> {
    use zune_jpeg::JpegDecoder;
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options);
    let pixels = decoder
        .decode()
        .map_err(|error| VncError::Protocol(format!("a Tight JPEG picture: {error:?}")))?;
    let size = decoder
        .info()
        .map(|info| (info.width, info.height))
        .unwrap_or_default();
    if size != (area.width, area.height) || pixels.len() != area.pixels() * 4 {
        return Err(VncError::Protocol(format!(
            "a Tight JPEG picture of {}x{} for a {}x{} rectangle",
            size.0, size.1, area.width, area.height
        )));
    }
    Ok(pixels)
}

/// A slice read from the front.
struct Bytes<'a> {
    data: &'a [u8],
}

impl<'a> Bytes<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], VncError> {
        if self.data.len() < len {
            return Err(VncError::Protocol("a ZRLE rectangle ended early".into()));
        }
        let (taken, rest) = self.data.split_at(len);
        self.data = rest;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, VncError> {
        Ok(self.take(1)?[0])
    }

    fn cpixel(&mut self) -> Result<[u8; 3], VncError> {
        let bytes = self.take(3)?;
        Ok([bytes[0], bytes[1], bytes[2]])
    }

    /// A run length: bytes of 255 add up, the last one ends it; plus one.
    fn run(&mut self) -> Result<usize, VncError> {
        let mut length = 1;
        loop {
            let byte = self.byte()?;
            length += usize::from(byte);
            if byte != 255 {
                return Ok(length);
            }
        }
    }
}

fn zrle_tile(input: &mut Bytes<'_>, canvas: &mut Canvas, tile: Area) -> Result<(), VncError> {
    let pixels = tile.pixels();
    let kind = input.byte()?;
    match kind {
        0 => canvas.put_rgb(tile, input.take(pixels * 3)?),
        1 => canvas.fill(tile, input.cpixel()?),
        2..=16 => {
            let size = usize::from(kind);
            let palette: Vec<[u8; 3]> = (0..size)
                .map(|_| input.cpixel())
                .collect::<Result<_, _>>()?;
            let bits = match size {
                2 => 1,
                3..=4 => 2,
                _ => 4,
            };
            let row = (usize::from(tile.width) * bits).div_ceil(8);
            let packed = input.take(row * usize::from(tile.height))?;
            let mut rgb = Vec::with_capacity(pixels * 3);
            for y in 0..usize::from(tile.height) {
                for x in 0..usize::from(tile.width) {
                    let bit = x * bits;
                    let byte = packed[y * row + bit / 8];
                    let shift = 8 - bits - (bit % 8);
                    let index = usize::from((byte >> shift) & ((1 << bits) - 1));
                    let colour = palette.get(index).ok_or_else(|| {
                        VncError::Protocol("a ZRLE palette index out of range".into())
                    })?;
                    rgb.extend_from_slice(colour);
                }
            }
            canvas.put_rgb(tile, &rgb);
        }
        128 => {
            let mut rgb = Vec::with_capacity(pixels * 3);
            while rgb.len() < pixels * 3 {
                let colour = input.cpixel()?;
                let run = input.run()?;
                for _ in 0..run.min(pixels - rgb.len() / 3) {
                    rgb.extend_from_slice(&colour);
                }
            }
            canvas.put_rgb(tile, &rgb);
        }
        130..=255 => {
            let size = usize::from(kind - 128);
            let palette: Vec<[u8; 3]> = (0..size)
                .map(|_| input.cpixel())
                .collect::<Result<_, _>>()?;
            let mut rgb = Vec::with_capacity(pixels * 3);
            while rgb.len() < pixels * 3 {
                let index = input.byte()?;
                let colour = palette.get(usize::from(index & 0x7F)).ok_or_else(|| {
                    VncError::Protocol("a ZRLE palette index out of range".into())
                })?;
                let run = if index & 0x80 != 0 { input.run()? } else { 1 };
                for _ in 0..run.min(pixels - rgb.len() / 3) {
                    rgb.extend_from_slice(colour);
                }
            }
            canvas.put_rgb(tile, &rgb);
        }
        other => {
            return Err(VncError::Protocol(format!(
                "unknown ZRLE tile type {other}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    fn area(x: u16, y: u16, width: u16, height: u16) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    /// A zlib stream that the decoder keeps: each part flushed (Z_SYNC_FLUSH), as servers do.
    struct Zlib(flate2::Compress);

    impl Zlib {
        fn new() -> Self {
            Self(flate2::Compress::new(flate2::Compression::default(), true))
        }

        fn part(&mut self, data: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(data.len() + 64);
            self.0
                .compress_vec(data, &mut out, flate2::FlushCompress::Sync)
                .unwrap();
            out
        }
    }

    #[tokio::test]
    async fn raw_and_copy_rect() {
        let mut canvas = Canvas::new(4, 4);
        let pixels: Vec<u8> = [[10, 20, 30, 0], [40, 50, 60, 0]].concat();
        Decoders::raw(&mut pixels.as_slice(), &mut canvas, area(0, 0, 2, 1))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(1, 0), [40, 50, 60]);
        Decoders::copy_rect(
            &mut [0_u8, 0, 0, 0].as_slice(),
            &mut canvas,
            area(2, 3, 2, 1),
        )
        .await
        .unwrap();
        assert_eq!(canvas.pixel(2, 3), [10, 20, 30]);
        assert!(
            Decoders::copy_rect(
                &mut [0_u8, 3, 0, 3].as_slice(),
                &mut canvas,
                area(0, 0, 2, 2)
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn hextile() {
        let mut canvas = Canvas::new(20, 16);
        let mut data = Vec::new();
        // Tile 1 (16x16): a red background, a green foreground, one subrectangle 2x3 at 1,1.
        data.extend([2 | 4 | 8, 255, 0, 0, 0, 0, 255, 0, 0, 1, 0x11, 0x12]);
        // Tile 2 (4x16): raw.
        data.push(1);
        for _ in 0..4 * 16 {
            data.extend([0, 0, 255, 0]);
        }
        Decoders::hextile(&mut data.as_slice(), &mut canvas, area(0, 0, 20, 16))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(0, 0), [255, 0, 0]);
        assert_eq!(canvas.pixel(2, 3), [0, 255, 0]);
        assert_eq!(canvas.pixel(3, 1), [255, 0, 0]);
        assert_eq!(canvas.pixel(19, 15), [0, 0, 255]);
        // A subrectangle outside its tile.
        let bad = [8_u8, 1, 0xF0, 0x10];
        assert!(
            Decoders::hextile(&mut bad.as_slice(), &mut canvas, area(0, 0, 16, 16))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn zrle_tiles() {
        let mut zlib = Zlib::new();
        let mut decoders = Decoders::default();
        let mut canvas = Canvas::new(70, 2);
        // A 70x2 rectangle: tile 64x2 as plain runs, then 6x2 as a two-colour packed palette.
        let mut tiles = vec![128];
        tiles.extend([1, 2, 3, 99]); // 100 pixels of (1, 2, 3)
        tiles.extend([4, 5, 6, 27]); // 28 more of (4, 5, 6)
        tiles.extend([2, 9, 9, 9, 7, 7, 7]);
        tiles.extend([0b1010_1000, 0b0101_0100]);
        let block = zlib.part(&tiles);
        let mut data = u32::try_from(block.len()).unwrap().to_be_bytes().to_vec();
        data.extend(block);
        decoders
            .zrle(&mut data.as_slice(), &mut canvas, area(0, 0, 70, 2))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(63, 0), [1, 2, 3]);
        assert_eq!(canvas.pixel(35, 1), [1, 2, 3]);
        assert_eq!(canvas.pixel(36, 1), [4, 5, 6]);
        assert_eq!(canvas.pixel(64, 0), [7, 7, 7]);
        assert_eq!(canvas.pixel(65, 0), [9, 9, 9]);
        assert_eq!(canvas.pixel(64, 1), [9, 9, 9]);
        // The stream goes on in the next rectangle: one solid tile.
        let block = zlib.part(&[1, 50, 60, 70]);
        let mut data = u32::try_from(block.len()).unwrap().to_be_bytes().to_vec();
        data.extend(block);
        decoders
            .zrle(&mut data.as_slice(), &mut canvas, area(0, 0, 4, 1))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(3, 0), [50, 60, 70]);
    }

    #[tokio::test]
    async fn tight_fill_copy_palette_gradient_and_jpeg() {
        let mut decoders = Decoders::default();
        let mut canvas = Canvas::new(16, 16);
        // Fill.
        Decoders::default()
            .tight(
                &mut [0x80_u8, 1, 2, 3].as_slice(),
                &mut canvas,
                area(0, 0, 16, 16),
            )
            .await
            .unwrap();
        assert_eq!(canvas.pixel(15, 15), [1, 2, 3]);
        // Copy, below 12 bytes: in clear (2x1 pixels).
        let small = [0x00_u8, 10, 11, 12, 13, 14, 15];
        decoders
            .tight(&mut small.as_slice(), &mut canvas, area(0, 0, 2, 1))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(1, 0), [13, 14, 15]);
        // Copy, compressed on stream 1 (8x2 pixels = 48 bytes).
        let mut zlib = Zlib::new();
        let pixels: Vec<u8> = (0..48).collect();
        let block = zlib.part(&pixels);
        let mut data = vec![0x10];
        data.push(u8::try_from(block.len()).unwrap());
        data.extend(&block);
        decoders
            .tight(&mut data.as_slice(), &mut canvas, area(0, 2, 8, 2))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(7, 3), [45, 46, 47]);
        // Palette of two colours, one bit a pixel (4x2: 2 bytes, in clear).
        let data = [
            0x40_u8,
            1,
            1,
            0,
            0,
            0,
            255,
            255,
            255,
            0b1001_0000,
            0b0110_0000,
        ];
        decoders
            .tight(&mut data.as_slice(), &mut canvas, area(0, 4, 4, 2))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(0, 4), [255, 255, 255]);
        assert_eq!(canvas.pixel(1, 4), [0, 0, 0]);
        assert_eq!(canvas.pixel(1, 5), [255, 255, 255]);
        // Gradient (2x2 = 12 bytes, compressed on stream 0, reset first).
        let wanted = [10_u8, 10, 10, 20, 20, 20, 30, 30, 30, 40, 40, 40];
        let mut differences = Vec::new();
        for (index, value) in wanted.iter().enumerate() {
            let predicted = match index / 3 {
                0 => 0,
                1 => 10,
                2 => 10,
                _ => 20 + 30 - 10,
            };
            differences.push(value.wrapping_sub(predicted));
        }
        let mut zlib = Zlib::new();
        let block = zlib.part(&differences);
        let mut data = vec![0x41, 2, u8::try_from(block.len()).unwrap()];
        data.extend(&block);
        decoders
            .tight(&mut data.as_slice(), &mut canvas, area(8, 8, 2, 2))
            .await
            .unwrap();
        assert_eq!(canvas.pixel(9, 9), [40, 40, 40]);
        assert_eq!(canvas.pixel(8, 9), [30, 30, 30]);
        // JPEG: a picture that doesn't decode is an error, not a crash.
        let data = [0x90_u8, 3, 0xFF, 0xD8, 0x00];
        assert!(
            decoders
                .tight(&mut data.as_slice(), &mut canvas, area(0, 0, 8, 8))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn cursor_with_its_mask() {
        let area = area(1, 2, 2, 1);
        let mut data: Vec<u8> = [[255, 0, 0, 0], [0, 255, 0, 0]].concat();
        data.push(0b1000_0000);
        let cursor = Decoders::cursor(&mut data.as_slice(), area).await.unwrap();
        assert_eq!((cursor.hot_x, cursor.hot_y), (1, 2));
        assert_eq!(cursor.rgba, vec![255, 0, 0, 255, 0, 255, 0, 0]);
    }

    #[test]
    fn gradient_predicts_from_the_neighbours() {
        // One row: each pixel is the left one plus its difference.
        assert_eq!(gradient(&[5, 5, 5, 1, 2, 3], 2), vec![5, 5, 5, 6, 7, 8]);
    }
}
