//! The remote framebuffer as RGBA, which the decoders draw into. CopyRect and the cursor need the
//! pixels already drawn, so the session keeps the whole desktop here and hands the app the
//! rectangles that changed.

use crate::VncError;

/// A rectangle of the desktop, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Area {
    /// Left.
    pub x: u16,
    /// Top.
    pub y: u16,
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
}

impl Area {
    /// The area's pixel count.
    #[must_use]
    pub fn pixels(self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }
}

/// The desktop's pixels, RGBA, `width * 4` bytes a row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Canvas {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
}

impl Canvas {
    /// A black desktop of this size.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let mut canvas = Self::default();
        canvas.resize(width, height);
        canvas
    }

    /// Width.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// All the pixels.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// A new size, black.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.rgba.clear();
        self.rgba
            .resize(usize::from(width) * usize::from(height) * 4, 0);
        for pixel in self.rgba.chunks_exact_mut(4) {
            pixel[3] = 0xFF;
        }
    }

    /// Fails unless `area` lies inside the desktop.
    ///
    /// # Errors
    ///
    /// [`VncError::Protocol`] for a rectangle the server shouldn't send.
    pub fn check(&self, area: Area) -> Result<(), VncError> {
        let inside = u32::from(area.x) + u32::from(area.width) <= u32::from(self.width)
            && u32::from(area.y) + u32::from(area.height) <= u32::from(self.height);
        if inside {
            Ok(())
        } else {
            Err(VncError::Protocol(format!(
                "a rectangle outside the desktop ({}x{} at {},{} on {}x{})",
                area.width, area.height, area.x, area.y, self.width, self.height
            )))
        }
    }

    fn offset(&self, x: u16, y: u16) -> usize {
        (usize::from(y) * usize::from(self.width) + usize::from(x)) * 4
    }

    /// Paints `area` (already checked) with one colour.
    pub fn fill(&mut self, area: Area, rgb: [u8; 3]) {
        let pixel = [rgb[0], rgb[1], rgb[2], 0xFF];
        for row in 0..area.height {
            let start = self.offset(area.x, area.y + row);
            let end = start + usize::from(area.width) * 4;
            for target in self.rgba[start..end].chunks_exact_mut(4) {
                target.copy_from_slice(&pixel);
            }
        }
    }

    /// Paints `area` (already checked) with `rgb`, three bytes a pixel, row after row.
    pub fn put_rgb(&mut self, area: Area, rgb: &[u8]) {
        let width = usize::from(area.width);
        for (row, line) in rgb
            .chunks_exact(width * 3)
            .take(usize::from(area.height))
            .enumerate()
        {
            let start = self.offset(area.x, area.y + u16::try_from(row).unwrap_or(u16::MAX));
            for (target, source) in self.rgba[start..start + width * 4]
                .chunks_exact_mut(4)
                .zip(line.chunks_exact(3))
            {
                target[..3].copy_from_slice(source);
                target[3] = 0xFF;
            }
        }
    }

    /// Paints `area` (already checked) with `rgba`, four bytes a pixel (the fourth ignored).
    pub fn put_rgbx(&mut self, area: Area, rgbx: &[u8]) {
        let width = usize::from(area.width);
        for (row, line) in rgbx
            .chunks_exact(width * 4)
            .take(usize::from(area.height))
            .enumerate()
        {
            let start = self.offset(area.x, area.y + u16::try_from(row).unwrap_or(u16::MAX));
            let target = &mut self.rgba[start..start + width * 4];
            target.copy_from_slice(line);
            for pixel in target.chunks_exact_mut(4) {
                pixel[3] = 0xFF;
            }
        }
    }

    /// Copies the pixels at `from` to `area` (both already checked), as CopyRect: the source is
    /// read before anything is written, so overlapping areas work.
    pub fn copy(&mut self, from: (u16, u16), area: Area) {
        let source = self.read(Area {
            x: from.0,
            y: from.1,
            width: area.width,
            height: area.height,
        });
        self.put_rgbx(area, &source);
    }

    /// The pixels of `area` (already checked), RGBA.
    #[must_use]
    pub fn read(&self, area: Area) -> Vec<u8> {
        let width = usize::from(area.width) * 4;
        let mut out = Vec::with_capacity(width * usize::from(area.height));
        for row in 0..area.height {
            let start = self.offset(area.x, area.y + row);
            out.extend_from_slice(&self.rgba[start..start + width]);
        }
        out
    }

    /// One pixel's colour (tests).
    #[must_use]
    pub fn pixel(&self, x: u16, y: u16) -> [u8; 3] {
        let at = self.offset(x, y);
        [self.rgba[at], self.rgba[at + 1], self.rgba[at + 2]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_puts_copies_and_reads() {
        let mut canvas = Canvas::new(8, 4);
        assert_eq!(canvas.pixel(7, 3), [0, 0, 0]);
        let area = Area {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };
        canvas.fill(area, [1, 2, 3]);
        assert_eq!(canvas.pixel(2, 2), [1, 2, 3]);
        canvas.put_rgb(
            Area {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            &[9, 9, 9, 8, 8, 8],
        );
        assert_eq!(canvas.pixel(1, 0), [8, 8, 8]);
        // Overlapping: the source is read first.
        canvas.copy(
            (0, 0),
            Area {
                x: 1,
                y: 0,
                width: 3,
                height: 3,
            },
        );
        assert_eq!(canvas.pixel(1, 0), [9, 9, 9]);
        assert_eq!(canvas.pixel(2, 0), [8, 8, 8]);
        assert_eq!(canvas.pixel(3, 2), [1, 2, 3]);
        assert_eq!(canvas.read(area).len(), 16);
        assert!(
            canvas
                .check(Area {
                    x: 7,
                    y: 0,
                    width: 2,
                    height: 1
                })
                .is_err()
        );
        assert!(canvas.rgba().chunks_exact(4).all(|pixel| pixel[3] == 0xFF));
    }
}
