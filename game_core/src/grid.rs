use std::borrow::Borrow;

use bevy::prelude::*;

use crate::prelude::SSS;

pub const CELL_SIZE: f32 = 32.;

pub const CARDINAL_DIRECTIONS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
pub const ALL_DIRECTIONS: [(i32, i32); 8] = [
    (0, 1), (1, 0), (0, -1), (-1, 0), // Cardinal directions
    (1, 1), (1, -1), (-1, 1), (-1, -1) // Diagonal directions
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Bounds {
    pub width: i32,
    pub height: i32,
}
impl Bounds {
    pub const fn new(width: i32, height: i32) -> Self { Self { width, height } }
    pub const fn area(self) -> usize { (self.width * self.height) as usize }
    /// Width and height as unsigned, for shader uniforms that store grid dimensions.
    pub const fn as_u32(self) -> (u32, u32) { (self.width as u32, self.height as u32) }

    pub fn contains(self, coords: GridCoords) -> bool {
        coords.x >= 0 && coords.x < self.width && coords.y >= 0 && coords.y < self.height
    }
    /// True if a rectangle starting at `origin` with dimensions from `other` fits entirely inside `self`.
    pub fn contains_other(self, origin: GridCoords, other: impl Into<Bounds>) -> bool {
        let other = other.into();
        origin.x >= 0 && origin.x + other.width <= self.width
            && origin.y >= 0 && origin.y + other.height <= self.height
    }
    pub fn index(self, coords: GridCoords) -> usize {
        (coords.y * self.width + coords.x) as usize
    }
    pub fn index_checked(self, coords: GridCoords) -> Option<usize> {
        self.contains(coords).then(|| self.index(coords))
    }
    pub fn cardinal_neighbors(self, coords: GridCoords) -> impl Iterator<Item = GridCoords> {
        CARDINAL_DIRECTIONS.iter()
            .map(move |&(dx, dy)| coords.shifted((dx, dy)))
            .filter(move |c| self.contains(*c))
    }
    pub fn all_neighbors(self, coords: GridCoords) -> impl Iterator<Item = GridCoords> {
        ALL_DIRECTIONS.iter()
            .map(move |&(dx, dy)| coords.shifted((dx, dy)))
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

/// Converts a grid cell count to world-space size.
fn world_size_of_cells(width: i32, height: i32) -> Vec2 {
    Vec2::new(width as f32 * CELL_SIZE, height as f32 * CELL_SIZE)
}

/// Shared map identity and dimensions, used to size grids and position the camera.
#[derive(Resource, Default, Clone, SSS)]
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
        world_size_of_cells(self.grid_bounds.width, self.grid_bounds.height)
    }
}

// This component should be replaced in full. Mutate in place only if you are sure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Component, Hash)]
pub struct GridCoords {
    pub x: i32,
    pub y: i32,
}
impl GridCoords {
    pub fn from_transform(transform: &Transform) -> Self {
        Self {
            x: (transform.translation.x / CELL_SIZE).floor() as i32,
            y: (transform.translation.y / CELL_SIZE).floor() as i32,
        }
    }
    pub fn from_world_vec2(world_coords: Vec2) -> Self {
        Self {
            x: (world_coords.x / CELL_SIZE).floor() as i32,
            y: (world_coords.y / CELL_SIZE).floor() as i32,
        }
    }
    pub fn are_in_bounds(&self, bounds: impl Into<Bounds>) -> bool {
        bounds.into().contains(*self)
    }
    pub fn to_world_position(&self) -> Vec2 {
        Vec2::new(self.x as f32 * CELL_SIZE, self.y as f32 * CELL_SIZE)
    }
    pub fn to_world_position_centered(&self, imprint: impl Borrow<GridImprint>) -> Vec2 {
        self.to_world_position() + imprint.borrow().world_center()
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


#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum GridImprint {
    Rectangle { width: i32, height: i32 },
    /// Plus/cross shape. `extents` cells in each cardinal direction from the center.
    /// Bounding box is (2*extents+1) × (2*extents+1); `origin` is its bottom-left corner.
    Plus { extents: i32 },
}
impl GridImprint {
    /// Iterates all cells covered by the imprint at `origin`. Does not check map bounds.
    pub fn iter(&self, origin: GridCoords) -> GridImprintIter {
        match self {
            GridImprint::Rectangle { width, height } => GridImprintIter::Rectangle {
                origin, width: *width, height: *height, index: 0,
            },
            GridImprint::Plus { extents } => GridImprintIter::Plus {
                origin, extents: *extents, index: 0,
            },
        }
    }
    /// Iterates cells covered by the imprint at `origin`, skipping any that fall outside `bounds`.
    pub fn iter_in_bounds(&self, origin: GridCoords, bounds: impl Into<Bounds>) -> impl Iterator<Item = GridCoords> {
        let bounds = bounds.into();
        self.iter(origin).filter(move |coords| coords.are_in_bounds(bounds))
    }
    /// True if the imprint placed at `origin` fits entirely within `bounds`.
    pub fn is_in_bounds(&self, origin: GridCoords, bounds: impl Into<Bounds>) -> bool {
        bounds.into().contains_other(origin, self)
    }

    pub fn covers_coords(&self, origin: GridCoords, coords: GridCoords) -> bool {
        match self {
            GridImprint::Rectangle { width, height } => {
                coords.x >= origin.x && coords.x < origin.x + *width && coords.y >= origin.y && coords.y < origin.y + *height
            }
            GridImprint::Plus { extents } => {
                let e = *extents;
                let side = 2 * e + 1;
                let in_bb = coords.x >= origin.x && coords.x < origin.x + side
                    && coords.y >= origin.y && coords.y < origin.y + side;
                if !in_bb { return false; }
                let lx = coords.x - origin.x;
                let ly = coords.y - origin.y;
                lx == e || ly == e
            }
        }
    }
    pub fn world_size(&self) -> Vec2 {
        match self {
            GridImprint::Rectangle { width, height } => world_size_of_cells(*width, *height),
            GridImprint::Plus { extents } => {
                let side = 2 * *extents + 1;
                world_size_of_cells(side, side)
            }
        }
    }
    pub fn world_center(&self) -> Vec2 {
        self.world_size() / 2.
    }

    /// Short footprint label: `3×3` for a rectangle, and a plus's bounding box as `Plus 3×3`.
    pub fn label(&self) -> String {
        match self {
            GridImprint::Rectangle { width, height } => format!("{width}×{height}"),
            GridImprint::Plus { extents } => {
                let span = extents * 2 + 1;
                format!("Plus {span}×{span}")
            }
        }
    }

    /// Generate a random local offset within the imprint bounds.
    pub fn random_local_offset(&self) -> Vec2 {
        use nanorand::Rng;
        let mut rng = nanorand::tls_rng();
        match self {
            GridImprint::Rectangle { width, height } => {
                Vec2::new(
                    rng.generate::<f32>() * *width as f32 * CELL_SIZE,
                    rng.generate::<f32>() * *height as f32 * CELL_SIZE,
                )
            }
            GridImprint::Plus { extents } => {
                let e = *extents;
                let total = 4 * e + 1;
                let cell_index = (rng.generate::<f32>() * total as f32) as i32;
                let (cell_x, cell_y) = if cell_index < 2 * e + 1 {
                    (e, cell_index)
                } else {
                    let row_index = cell_index - (2 * e + 1);
                    let x = if row_index < e { row_index } else { row_index + 1 };
                    (x, e)
                };
                Vec2::new(
                    cell_x as f32 * CELL_SIZE + rng.generate::<f32>() * CELL_SIZE,
                    cell_y as f32 * CELL_SIZE + rng.generate::<f32>() * CELL_SIZE,
                )
            }
        }
    }
}

impl Default for GridImprint {
    fn default() -> Self {
        GridImprint::Rectangle { width: 1, height: 1 }
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

pub enum GridImprintIter {
    Rectangle { origin: GridCoords, width: i32, height: i32, index: i32 },
    Plus { origin: GridCoords, extents: i32, index: i32 },
}
impl Iterator for GridImprintIter {
    type Item = GridCoords;
    fn next(&mut self) -> Option<GridCoords> {
        match self {
            GridImprintIter::Rectangle { origin, width, height, index } => {
                if *index >= *width * *height { return None; }
                let x = *index % *width;
                let y = *index / *width;
                *index += 1;
                Some(origin.shifted((x, y)))
            }
            GridImprintIter::Plus { origin, extents, index } => {
                let e = *extents;
                let col_count = 2 * e + 1;
                let total = col_count + 2 * e; // col + row minus already-counted center
                if *index >= total { return None; }
                let result = if *index < col_count {
                    origin.shifted((e, *index))
                } else {
                    let row_index = *index - col_count;
                    let x = if row_index < e { row_index } else { row_index + 1 };
                    origin.shifted((x, e))
                };
                *index += 1;
                Some(result)
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = match self {
            GridImprintIter::Rectangle { width, height, index, .. } => {
                ((*width * *height) - *index).max(0) as usize
            }
            GridImprintIter::Plus { extents, index, .. } => {
                ((4 * *extents + 1) - *index).max(0) as usize
            }
        };
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for GridImprintIter {}
