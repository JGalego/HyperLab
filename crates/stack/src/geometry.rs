//! Card-space geometry.
//!
//! All coordinates are integers in card space, with the origin at the top-left
//! corner of the card. Integers keep the classic renderer pixel-crisp.

use serde::{Deserialize, Serialize};

/// A point in card space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Point {
    /// Distance from the left edge of the card.
    pub x: i32,
    /// Distance from the top edge of the card.
    pub y: i32,
}

impl Point {
    /// Creates a point.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    /// Horizontal extent, never negative.
    pub width: i32,
    /// Vertical extent, never negative.
    pub height: i32,
}

impl Size {
    /// Creates a size, clamping negative extents to zero.
    #[must_use]
    pub const fn new(width: i32, height: i32) -> Self {
        Self {
            width: if width < 0 { 0 } else { width },
            height: if height < 0 { 0 } else { height },
        }
    }
}

impl Default for Size {
    /// The size of a classic HyperCard card, which HyperLab keeps as its
    /// default so that classic-looking stacks need no configuration.
    fn default() -> Self {
        Self::new(512, 342)
    }
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Distance from the left edge of the card.
    pub left: i32,
    /// Distance from the top edge of the card.
    pub top: i32,
    /// Horizontal extent, never negative.
    pub width: i32,
    /// Vertical extent, never negative.
    pub height: i32,
}

impl Rect {
    /// Creates a rectangle, clamping negative extents to zero.
    #[must_use]
    pub const fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            width: if width < 0 { 0 } else { width },
            height: if height < 0 { 0 } else { height },
        }
    }

    /// The x coordinate of the right edge.
    #[must_use]
    pub const fn right(&self) -> i32 {
        self.left + self.width
    }

    /// The y coordinate of the bottom edge.
    #[must_use]
    pub const fn bottom(&self) -> i32 {
        self.top + self.height
    }

    /// The top-left corner.
    #[must_use]
    pub const fn origin(&self) -> Point {
        Point::new(self.left, self.top)
    }

    /// The width and height.
    #[must_use]
    pub const fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// A copy moved by `dx` and `dy`.
    #[must_use]
    pub const fn translated(&self, dx: i32, dy: i32) -> Self {
        Self::new(self.left + dx, self.top + dy, self.width, self.height)
    }

    /// A copy with a new origin.
    #[must_use]
    pub const fn moved_to(&self, origin: Point) -> Self {
        Self::new(origin.x, origin.y, self.width, self.height)
    }

    /// A copy with a new size.
    #[must_use]
    pub const fn resized(&self, size: Size) -> Self {
        Self::new(self.left, self.top, size.width, size.height)
    }

    /// Whether `point` lies inside the rectangle.
    ///
    /// The left and top edges are inside; the right and bottom edges are not,
    /// so adjacent rectangles never both claim the same pixel.
    #[must_use]
    pub const fn contains(&self, point: Point) -> bool {
        point.x >= self.left
            && point.x < self.right()
            && point.y >= self.top
            && point.y < self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_extents_are_clamped() {
        let rect = Rect::new(10, 10, -5, 20);
        assert_eq!(rect.width, 0);
        assert_eq!(rect.height, 20);
    }

    #[test]
    fn edges_are_derived_from_origin_and_size() {
        let rect = Rect::new(10, 20, 30, 40);
        assert_eq!(rect.right(), 40);
        assert_eq!(rect.bottom(), 60);
    }

    #[test]
    fn containment_excludes_the_far_edges() {
        let rect = Rect::new(0, 0, 10, 10);
        assert!(rect.contains(Point::new(0, 0)));
        assert!(rect.contains(Point::new(9, 9)));
        assert!(!rect.contains(Point::new(10, 5)));
        assert!(!rect.contains(Point::new(-1, 5)));
    }

    #[test]
    fn transforms_preserve_the_other_axis() {
        let rect = Rect::new(1, 2, 3, 4);
        assert_eq!(rect.translated(10, 0), Rect::new(11, 2, 3, 4));
        assert_eq!(rect.resized(Size::new(8, 9)), Rect::new(1, 2, 8, 9));
    }
}
