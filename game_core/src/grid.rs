use bevy::prelude::*;
use serde::Serialize;

use crate::prelude::SSS;

pub const CELL_SIZE: f32 = 32.;

const CARDINAL_DIRECTIONS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const ALL_DIRECTIONS: [(i32, i32); 8] = [
    (0, 1), (1, 0), (0, -1), (-1, 0), // Cardinal directions
    (1, 1), (1, -1), (-1, 1), (-1, -1) // Diagonal directions
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
pub struct Bounds {
    pub width: i32,
    pub height: i32,
}
impl Bounds {
    pub const fn new(width: i32, height: i32) -> Self { Self { width, height } }
    pub const fn area(self) -> usize { (self.width * self.height) as usize }

    pub fn contains(self, coords: GridCoords) -> bool {
        coords.x >= 0 && coords.x < self.width && coords.y >= 0 && coords.y < self.height
    }
    /// Converts a cell count to world-space size.
    pub fn world_size(self) -> Vec2 {
        Vec2::new(self.width as f32 * CELL_SIZE, self.height as f32 * CELL_SIZE)
    }
    pub fn index(self, coords: GridCoords) -> usize {
        (coords.y * self.width + coords.x) as usize
    }
    pub fn index_checked(self, coords: GridCoords) -> Option<usize> {
        self.contains(coords).then(|| self.index(coords))
    }
    pub fn cardinal_neighbors(self, coords: GridCoords) -> impl Iterator<Item = GridCoords> {
        CARDINAL_DIRECTIONS.iter()
            .map(move |&direction| coords.shifted(direction))
            .filter(move |c| self.contains(*c))
    }
    pub fn all_neighbors(self, coords: GridCoords) -> impl Iterator<Item = GridCoords> {
        ALL_DIRECTIONS.iter()
            .map(move |&direction| coords.shifted(direction))
            .filter(move |c| self.contains(*c))
    }
    /// Iterates every cell as local coords `(0,0)..(width-1,height-1)`, row-major with `x` fastest.
    /// The position of each item equals the [`Self::index`] of that cell, so `enumerate` yields indices.
    pub fn iter(self) -> impl Iterator<Item = GridCoords> {
        (0..self.height).flat_map(move |y| (0..self.width).map(move |x| GridCoords { x, y }))
    }
}

impl From<(i32, i32)> for Bounds {
    fn from((width, height): (i32, i32)) -> Self { Self { width, height } }
}
impl From<Bounds> for UVec2 {
    fn from(bounds: Bounds) -> Self {
        UVec2::new(bounds.width as u32, bounds.height as u32)
    }
}

/// Shared map identity and dimensions, used to size grids and position the camera.
#[derive(Resource, Default, Clone, SSS, Serialize)]
pub struct MapInfo {
    pub grid_bounds: Bounds,
    pub name: String,
}
impl MapInfo {
    pub fn new(name: impl Into<String>, grid_bounds: impl Into<Bounds>) -> Self {
        Self {
            grid_bounds: grid_bounds.into(),
            name: name.into(),
        }
    }
    pub fn world_size(&self) -> Vec2 {
        self.grid_bounds.world_size()
    }
}

/// Snaps the entity's `Transform` to the world center of its `GridArea`.
/// The snap happens when `GridArea` is inserted. Mutating `GridArea` in place does not snap.
#[derive(Component, Default)]
#[require(Transform)]
pub struct AutoGridSnap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct GridCoords {
    pub x: i32,
    pub y: i32,
}
impl GridCoords {
    pub fn from_transform(transform: &Transform) -> Self {
        Self::from_world_position(transform.translation.xy())
    }
    pub fn from_world_position(world_coords: Vec2) -> Self {
        Self {
            x: (world_coords.x / CELL_SIZE).floor() as i32,
            y: (world_coords.y / CELL_SIZE).floor() as i32,
        }
    }
    pub fn to_world_position(&self) -> Vec2 {
        Vec2::new(self.x as f32 * CELL_SIZE, self.y as f32 * CELL_SIZE)
    }
    pub fn shifted(&self, (dx, dy): (i32, i32)) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
    pub fn manhattan_distance(&self, other: &Self) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}
impl From<(i32, i32)> for GridCoords {
    fn from(coords: (i32, i32)) -> Self {
        Self {
            x: coords.0,
            y: coords.1,
        }
    }
}
impl From<GridCoords> for (i32, i32) {
    fn from(coords: GridCoords) -> Self {
        (coords.x, coords.y)
    }
}
impl From<GridCoords> for IVec2 {
    fn from(coords: GridCoords) -> Self {
        IVec2::new(coords.x, coords.y)
    }
}
impl std::fmt::Display for GridCoords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}


#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridImprint {
    Rectangle { width: i32, height: i32 },
    /// Plus/cross shape. `extents` cells in each cardinal direction from the center.
    /// Bounding box is (2*extents+1) × (2*extents+1).
    Plus { extents: i32 },
}
impl GridImprint {
    pub const fn square(side: i32) -> Self {
        GridImprint::Rectangle { width: side, height: side }
    }
    /// Square of `reach` cells in each direction around a middle cell.
    pub const fn square_of_reach(reach: i32) -> Self {
        Self::square(2 * reach + 1)
    }

    pub const fn cell_count(&self) -> i32 {
        match self {
            GridImprint::Rectangle { width, height } => *width * *height,
            GridImprint::Plus { extents } => 4 * *extents + 1,
        }
    }

    pub fn world_size(&self) -> Vec2 {
        Bounds::from(self).world_size()
    }
    pub fn world_center(&self) -> Vec2 {
        self.world_size() / 2.
    }

    /// Short imprint label: `3×3` for a rectangle, and a plus's bounding box as `Plus 3×3`.
    pub fn label(&self) -> String {
        let Bounds { width, height } = self.into();
        match self {
            GridImprint::Rectangle { .. } => format!("{width}×{height}"),
            GridImprint::Plus { .. } => format!("Plus {width}×{height}"),
        }
    }

    /// Local cell at `index` of a plus: the vertical bar bottom to top, then the horizontal bar left to right without its center.
    const fn plus_cell(extents: i32, index: i32) -> (i32, i32) {
        let column_length = 2 * extents + 1;
        if index < column_length { return (extents, index); }
        let row_index = index - column_length;
        let x = if row_index < extents { row_index } else { row_index + 1 };
        (x, extents)
    }
}

impl Default for GridImprint {
    fn default() -> Self {
        GridImprint::square(1)
    }
}
impl From<&GridImprint> for Bounds {
    fn from(imprint: &GridImprint) -> Self {
        match imprint {
            GridImprint::Rectangle { width, height } => Self { width: *width, height: *height },
            GridImprint::Plus { extents } => { let side = 2 * *extents + 1; Self { width: side, height: side } }
        }
    }
}
impl From<GridImprint> for Bounds {
    fn from(imprint: GridImprint) -> Self { Self::from(&imprint) }
}

/// Cells covered by `imprint` placed at `origin`.
/// This component should be replaced in full. Mutate in place only if you are sure.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridArea {
    /// Bottom-left cell of the imprint's bounding box.
    pub origin: GridCoords,
    pub imprint: GridImprint,
}
impl GridArea {
    pub const fn new(origin: GridCoords, imprint: GridImprint) -> Self {
        Self { origin, imprint }
    }
    pub fn cell(coords: GridCoords) -> Self {
        Self::new(coords, GridImprint::default())
    }
    /// Places `imprint` at the zero origin, in its own local space.
    pub fn local(imprint: GridImprint) -> Self {
        Self::new(GridCoords::default(), imprint)
    }
    /// Places `imprint` with `center` as its middle cell. An even side puts `center` just above or right of the middle.
    pub fn centered_on(center: GridCoords, imprint: GridImprint) -> Self {
        let Bounds { width, height } = imprint.into();
        Self::new(center.shifted((-(width / 2), -(height / 2))), imprint)
    }
    /// Smallest rectangle of cells that contains `rect`.
    pub fn covering(rect: Rect) -> Self {
        let (min, max) = (GridCoords::from_world_position(rect.min), GridCoords::from_world_position(rect.max));
        Self::new(min, GridImprint::Rectangle { width: max.x - min.x + 1, height: max.y - min.y + 1 })
    }

    /// The rectangle of cells around the area, at the same origin.
    pub fn bounding_box(&self) -> Self {
        match self.imprint {
            GridImprint::Rectangle { .. } => *self,
            GridImprint::Plus { extents } => Self::new(self.origin, GridImprint::square_of_reach(extents)),
        }
    }

    pub fn world_center(&self) -> Vec2 {
        self.origin.to_world_position() + self.imprint.world_center()
    }
    pub fn world_rect(&self) -> Rect {
        let min = self.origin.to_world_position();
        Rect::from_corners(min, min + self.imprint.world_size())
    }
    pub fn random_world_point(&self) -> Vec2 {
        use nanorand::Rng;
        let mut rng = nanorand::tls_rng();
        let local_offset = match self.imprint {
            GridImprint::Rectangle { .. } => Vec2::new(rng.generate(), rng.generate()) * self.imprint.world_size(),
            GridImprint::Plus { extents } => {
                let (cell_x, cell_y) = GridImprint::plus_cell(extents, rng.generate_range(0..self.imprint.cell_count()));
                Vec2::new(cell_x as f32 + rng.generate::<f32>(), cell_y as f32 + rng.generate::<f32>()) * CELL_SIZE
            }
        };
        self.origin.to_world_position() + local_offset
    }

    /// Iterates all cells of the area. Does not check map bounds.
    pub fn iter(&self) -> GridAreaIter {
        GridAreaIter { area: *self, x: 0, y: 0 }
    }
    /// Iterates cells of the area, skipping any that fall outside `bounds`.
    pub fn iter_in_bounds(&self, bounds: Bounds) -> impl Iterator<Item = GridCoords> {
        self.iter().filter(move |coords| bounds.contains(*coords))
    }
    /// True if the area fits entirely within `bounds`.
    pub fn fits_in(&self, bounds: Bounds) -> bool {
        let Bounds { width, height } = self.imprint.into();
        bounds.contains(self.origin) && bounds.contains(self.origin.shifted((width - 1, height - 1)))
    }

    pub fn contains(&self, coords: GridCoords) -> bool {
        let local = coords.shifted((-self.origin.x, -self.origin.y));
        if !Bounds::from(self.imprint).contains(local) { return false; }
        match self.imprint {
            GridImprint::Rectangle { .. } => true,
            GridImprint::Plus { extents } => local.x == extents || local.y == extents,
        }
    }
    /// Manhattan distance from `coords` to the nearest cell of the area. Zero when covered.
    pub fn distance_to(&self, coords: GridCoords) -> i32 {
        let origin = self.origin;
        let span_distance = |value: i32, start: i32, length: i32| (start - value).max(value - (start + length - 1)).max(0);
        match self.imprint {
            GridImprint::Rectangle { width, height } => {
                span_distance(coords.x, origin.x, width) + span_distance(coords.y, origin.y, height)
            }
            GridImprint::Plus { extents } => {
                let side = 2 * extents + 1;
                let to_vertical_bar = (coords.x - (origin.x + extents)).abs() + span_distance(coords.y, origin.y, side);
                let to_horizontal_bar = (coords.y - (origin.y + extents)).abs() + span_distance(coords.x, origin.x, side);
                to_vertical_bar.min(to_horizontal_bar)
            }
        }
    }

    /// Cells of `old` that `new` does not cover, and cells of `new` that `old` does not cover.
    pub fn diff(old: Self, new: Self) -> (impl Iterator<Item = GridCoords>, impl Iterator<Item = GridCoords>) {
        (old.iter().filter(move |cell| !new.contains(*cell)), new.iter().filter(move |cell| !old.contains(*cell)))
    }
}
impl IntoIterator for GridArea {
    type Item = GridCoords;
    type IntoIter = GridAreaIter;
    fn into_iter(self) -> GridAreaIter { self.iter() }
}
impl IntoIterator for &GridArea {
    type Item = GridCoords;
    type IntoIter = GridAreaIter;
    fn into_iter(self) -> GridAreaIter { self.iter() }
}

/// Walks the area's cells. Rectangle: row by row with `x` fastest.
/// Plus: the vertical bar bottom to top, then the horizontal bar left to right without its center.
pub struct GridAreaIter {
    area: GridArea,
    /// Rectangle: column. Plus: index for [`GridImprint::plus_cell`].
    x: i32,
    /// Rectangle: row. Unused for Plus.
    y: i32,
}
impl Iterator for GridAreaIter {
    type Item = GridCoords;
    fn next(&mut self) -> Option<GridCoords> {
        let origin = self.area.origin;
        match self.area.imprint {
            GridImprint::Rectangle { width, height } => {
                if self.y >= height { return None; }
                let result = origin.shifted((self.x, self.y));
                self.x += 1;
                if self.x == width {
                    self.x = 0;
                    self.y += 1;
                }
                Some(result)
            }
            GridImprint::Plus { extents } => {
                if self.x >= self.area.imprint.cell_count() { return None; }
                let result = origin.shifted(GridImprint::plus_cell(extents, self.x));
                self.x += 1;
                Some(result)
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = match self.area.imprint {
            GridImprint::Rectangle { width, height } => (height - self.y) * width - self.x,
            GridImprint::Plus { .. } => self.area.imprint.cell_count() - self.x,
        };
        (remaining as usize, Some(remaining as usize))
    }
}
impl ExactSizeIterator for GridAreaIter {}
