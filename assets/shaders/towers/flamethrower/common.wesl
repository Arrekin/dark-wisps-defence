// Values shared by the Flamethrower's shaders.

// Half a grid cell, in world units. Mirrors `CELL_SIZE / 2` in game_core/src/grid.rs.
const TILE_HALF: f32 = 16.0;

// Heat at which `fire_ramp` reaches yellow. The Flamethrower's fire stops there, so white stays free
// for other effects.
const HEAT_CAP: f32 = 0.86;

// Distance from the nozzle at which the fuel has fully caught.
const BREAKUP: f32 = 150.0;
