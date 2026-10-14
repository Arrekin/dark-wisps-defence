import package::shaders::common::core::CELL_SIZE;

// Shared constants for `flame_stream` shaders.

const TILE_HALF: f32 = CELL_SIZE * 0.5;

// Axial distance from the nozzle where the stream reaches full ignition.
const BREAKUP: f32 = 150.0;
