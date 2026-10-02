//! The remote screen, shared between the session (which writes what the server changed) and the
//! app (which draws it): RGBA pixels and the rectangles changed since the app last took them.
//! Only those rectangles are copied, here and into the app's textures.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// A rectangle, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    /// Left.
    pub x: u16,
    /// Top.
    pub y: u16,
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
}

impl Rect {
    fn right(self) -> u32 {
        u32::from(self.x) + u32::from(self.width)
    }

    fn bottom(self) -> u32 {
        u32::from(self.y) + u32::from(self.height)
    }

    /// The smallest rectangle around both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self {
            x,
            y,
            width: u16::try_from(right - u32::from(x)).unwrap_or(u16::MAX),
            height: u16::try_from(bottom - u32::from(y)).unwrap_or(u16::MAX),
        }
    }

    fn overlaps(self, other: Self) -> bool {
        u32::from(self.x) <= other.right()
            && u32::from(other.x) <= self.right()
            && u32::from(self.y) <= other.bottom()
            && u32::from(other.y) <= self.bottom()
    }
}

/// How many separate changed rectangles are kept before they are merged into one.
const MAX_DIRTY: usize = 64;

/// The remote screen.
#[derive(Debug, Default)]
pub struct Frame {
    width: u16,
    height: u16,
    /// RGBA, `width * 4` bytes a row.
    pixels: Vec<u8>,
    dirty: Vec<Rect>,
    /// Grows when the size changes, so the app makes its textures again.
    generation: u64,
}

impl Frame {
    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// RGBA pixels, `width * 4` bytes a row.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Grows with each change of size.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// A new size: black, and all of it changed.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.pixels = vec![0; usize::from(width) * usize::from(height) * 4];
        self.generation += 1;
        self.dirty = vec![Rect {
            x: 0,
            y: 0,
            width,
            height,
        }];
    }

    /// Copies `rect` from `source` (RGBA, the same size as the frame) and marks it changed.
    pub fn copy_from(&mut self, source: &[u8], rect: Rect) {
        let width = usize::from(self.width);
        let stride = width * 4;
        if source.len() < self.pixels.len() {
            return;
        }
        let x = usize::from(rect.x).min(width);
        let right = (usize::from(rect.x) + usize::from(rect.width)).min(width);
        let bottom = (usize::from(rect.y) + usize::from(rect.height)).min(usize::from(self.height));
        if right <= x {
            return;
        }
        for row in usize::from(rect.y)..bottom {
            let span = row * stride + x * 4..row * stride + right * 4;
            if let (Some(to), Some(from)) = (self.pixels.get_mut(span.clone()), source.get(span)) {
                to.copy_from_slice(from);
            }
        }
        self.mark(rect);
    }

    /// Writes `rgba` (`rect.width * 4` bytes a row) at `rect`, clipped to the frame, and marks
    /// it changed.
    pub fn copy_rect(&mut self, rect: Rect, rgba: &[u8]) {
        let width = usize::from(self.width);
        let from_stride = usize::from(rect.width) * 4;
        let x = usize::from(rect.x);
        let right = (x + usize::from(rect.width)).min(width);
        if right <= x || rgba.len() < from_stride * usize::from(rect.height) {
            return;
        }
        let bottom = (usize::from(rect.y) + usize::from(rect.height)).min(usize::from(self.height));
        for (index, row) in (usize::from(rect.y)..bottom).enumerate() {
            let to = row * width * 4 + x * 4..row * width * 4 + right * 4;
            let from = index * from_stride..index * from_stride + (right - x) * 4;
            if let (Some(to), Some(from)) = (self.pixels.get_mut(to), rgba.get(from)) {
                to.copy_from_slice(from);
            }
        }
        self.mark(rect);
    }

    /// Marks `rect` changed.
    pub fn mark(&mut self, rect: Rect) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        // Merge with an overlapping one, else keep it apart, up to a limit.
        if let Some(existing) = self
            .dirty
            .iter_mut()
            .find(|existing| existing.overlaps(rect))
        {
            *existing = existing.union(rect);
        } else if self.dirty.len() < MAX_DIRTY {
            self.dirty.push(rect);
        } else {
            let all = self
                .dirty
                .iter()
                .fold(rect, |all, existing| all.union(*existing));
            self.dirty = vec![all];
        }
    }

    /// The rectangles changed since the last call.
    pub fn take_dirty(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.dirty)
    }
}

/// A frame shared by the session and the app.
pub type SharedFrame = Arc<Mutex<Frame>>;

/// Locks a shared frame.
pub fn lock(frame: &SharedFrame) -> MutexGuard<'_, Frame> {
    frame.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_the_rectangle() {
        let mut frame = Frame::default();
        frame.resize(4, 3);
        assert_eq!(frame.take_dirty().len(), 1);
        let source: Vec<u8> = (0..4 * 3 * 4)
            .map(|i| u8::try_from(i).unwrap_or(0))
            .collect();
        frame.copy_from(
            &source,
            Rect {
                x: 1,
                y: 1,
                width: 2,
                height: 1,
            },
        );
        // Row 1, columns 1 and 2: bytes 20..28.
        assert_eq!(&frame.pixels()[20..28], &source[20..28]);
        assert!(frame.pixels()[16..20].iter().all(|b| *b == 0));
        assert!(frame.pixels()[28..32].iter().all(|b| *b == 0));
        assert_eq!(
            frame.take_dirty(),
            [Rect {
                x: 1,
                y: 1,
                width: 2,
                height: 1
            }]
        );
        assert!(frame.take_dirty().is_empty());
    }

    #[test]
    fn overlapping_rectangles_merge_and_many_become_one() {
        let mut frame = Frame::default();
        frame.resize(100, 100);
        frame.take_dirty();
        frame.mark(Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        });
        frame.mark(Rect {
            x: 5,
            y: 5,
            width: 10,
            height: 10,
        });
        frame.mark(Rect {
            x: 50,
            y: 50,
            width: 1,
            height: 1,
        });
        let dirty = frame.take_dirty();
        assert_eq!(dirty.len(), 2);
        assert_eq!(
            dirty[0],
            Rect {
                x: 0,
                y: 0,
                width: 15,
                height: 15
            }
        );
        for i in 0..=MAX_DIRTY {
            let at = u16::try_from(i).unwrap_or(0) * 2;
            frame.mark(Rect {
                x: at % 100,
                y: (at / 100) * 2,
                width: 1,
                height: 1,
            });
        }
        assert_eq!(frame.take_dirty().len(), 1);
    }
}
