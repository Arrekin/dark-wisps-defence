// Shared field index for ownership and boundary queries. fields_internal uploads the data.
// Ownership follows `IndexedField::claim` and `FieldIndex::owner_at` in fields/src so shaders
// use the same territories as gameplay. Lane shaders bind the shared GPU group automatically;
// custom materials opt in through visuals::shared_gpu.

import package::shaders::common::core::CELL_SIZE;
import package::shaders::common::map_info::{grid_contains, grid_index};

// Field order and types mirror `GpuFieldShape` in fields_internal/src/field.rs.
struct FieldShape {
    center: vec2<f32>,
    // 0 for a free slot.
    current_radius: f32,
    strength: f32,
    field_type: u32,
    _padding: u32,
};

// The field table, one row per slot.
@group(3) @binding(107) var<storage, read> field_shapes: array<FieldShape>;
// Start offset for each cell in `field_cell_entries`, followed by a final end offset.
@group(3) @binding(108) var<storage, read> field_cell_offsets: array<u32>;
// Candidate slots concatenated in cell order.
@group(3) @binding(109) var<storage, read> field_cell_entries: array<u32>;

// Mirrors `FieldType` in fields/src/field.rs.
const FIELD_FORCE: u32 = 0u;
const FIELD_CRUCIBLE: u32 = 1u;

const NO_FIELD: u32 = 0xffffffffu;

// Returns the start and end offsets for the point's cell in `field_cell_entries`.
// Returns an empty range outside the map.
fn field_candidates_at(world_position: vec2<f32>) -> vec2<u32> {
    let coords = vec2<i32>(floor(world_position / CELL_SIZE));
    if !grid_contains(coords) {
        return vec2<u32>(0u);
    }
    let cell = grid_index(coords);
    return vec2<u32>(field_cell_offsets[cell], field_cell_offsets[cell + 1u]);
}

// ── Ownership ────────────────────────────────────────────────────────────────

// Sentinel claim for an empty candidate.
const NO_CLAIM: f32 = 1.0e9;

struct FieldCandidate {
    // `NO_FIELD` for an empty candidate.
    slot: u32,
    // (distance / (radius * strength))², or `NO_CLAIM` for an empty candidate.
    claim: f32,
};

// Up to three containing fields, ordered by claim. `first` owns the point.
struct FieldOwners {
    first: FieldCandidate,
    second: FieldCandidate,
    third: FieldCandidate,
};

// Finds the owner and up to two runners-up using the same rule as `FieldIndex::owner_at`.
fn field_owners_at(world_position: vec2<f32>) -> FieldOwners {
    let none = FieldCandidate(NO_FIELD, NO_CLAIM);
    var owners = FieldOwners(none, none, none);
    let candidates = field_candidates_at(world_position);
    for (var entry = candidates.x; entry < candidates.y; entry++) {
        let slot = field_cell_entries[entry];
        let shape = field_shapes[slot];
        if shape.current_radius <= 0.0 { continue; }
        let to_point = world_position - shape.center;
        let distance_squared = dot(to_point, to_point);
        if distance_squared > shape.current_radius * shape.current_radius { continue; }
        let weight = shape.current_radius * shape.strength;
        let candidate = FieldCandidate(slot, distance_squared / (weight * weight));
        if candidate.claim < owners.first.claim {
            owners.third = owners.second;
            owners.second = owners.first;
            owners.first = candidate;
        } else if candidate.claim < owners.second.claim {
            owners.third = owners.second;
            owners.second = candidate;
        } else if candidate.claim < owners.third.claim {
            owners.third = candidate;
        }
    }
    return owners;
}

// ── Boundary ─────────────────────────────────────────────────────────────────

const NO_SEAM_DISTANCE: f32 = 1.0e9;

// Neighbouring field and estimated distance to its seam, in world pixels.
struct FieldSeam {
    // `NO_FIELD` if no seam was found.
    slot: u32,
    // `NO_SEAM_DISTANCE` if no seam was found.
    distance: f32,
};

// Distances from a point to the field's circular rim and nearby seams, in world pixels.
struct FieldBoundary {
    // Distance to the circular rim; negative outside it.
    rim: f32,
    // The two nearest seams, nearest first.
    first: FieldSeam,
    second: FieldSeam,
    // Distance to the edge of the field's area: the rim or the nearest seam, whichever is closer.
    edge: f32,
};

// Distance in world pixels to the nearer intersection of two field circles.
// Returns `NO_SEAM_DISTANCE` when the circles do not cross.
fn rims_crossing_distance(p: vec2<f32>, field: FieldShape, other: FieldShape) -> f32 {
    let radius = field.current_radius;
    let other_radius = other.current_radius;
    let apart = distance(field.center, other.center);
    if apart < 1.0e-3 || apart >= radius + other_radius || apart <= abs(radius - other_radius) {
        return NO_SEAM_DISTANCE;
    }
    let along = (radius * radius - other_radius * other_radius + apart * apart) / (2.0 * apart);
    let half_chord = sqrt(max(radius * radius - along * along, 0.0));
    let axis = (other.center - field.center) / apart;
    let middle = field.center + axis * along;
    let across = vec2<f32>(-axis.y, axis.x);
    return min(distance(p, middle + across * half_chord), distance(p, middle - across * half_chord));
}

// Estimates the distance from a point owned by `field` to its boundary with `other`.
// The boundary follows equal claims, or the other field's rim if its radius limits its claim.
fn seam_distance(p: vec2<f32>, field: FieldShape, other: FieldShape) -> f32 {
    let field_distance = max(distance(p, field.center), 1.0e-3);
    let other_distance = max(distance(p, other.center), 1.0e-3);
    let field_weight = field.current_radius * field.strength;
    let other_weight = other.current_radius * other.strength;
    let field_scaled = field_distance / field_weight;
    let other_scaled = other_distance / other_weight;
    if other_distance > other.current_radius && other_scaled <= field_scaled {
        return other_distance - other.current_radius;
    }
    // Divide the difference in scaled distances by its gradient magnitude to estimate seam distance.
    let gradient = (p - other.center) / (other_distance * other_weight) - (p - field.center) / (field_distance * field_weight);
    let steepness = max(length(gradient), 1.0e-6);
    let across = max(other_scaled - field_scaled, 0.0) / steepness;
    let foot = p - gradient / steepness * across;
    if distance(foot, other.center) <= other.current_radius && distance(foot, field.center) <= field.current_radius {
        return across;
    }
    // The seam ends where the two rims cross.
    return rims_crossing_distance(p, field, other);
}

// Finds the field's rim and two nearest seams among candidates in the point's cell.
// The index margin includes nearby fields; distant neighbours may be absent.
fn field_boundary(slot: u32, world_position: vec2<f32>) -> FieldBoundary {
    let none = FieldSeam(NO_FIELD, NO_SEAM_DISTANCE);
    let field = field_shapes[slot];
    var boundary = FieldBoundary(field.current_radius - distance(world_position, field.center), none, none, 0.0);
    let candidates = field_candidates_at(world_position);
    for (var entry = candidates.x; entry < candidates.y; entry++) {
        let other_slot = field_cell_entries[entry];
        let other = field_shapes[other_slot];
        if other_slot == slot || other.current_radius <= 0.0 { continue; }
        let seam = FieldSeam(other_slot, seam_distance(world_position, field, other));
        if seam.distance < boundary.first.distance {
            boundary.second = boundary.first;
            boundary.first = seam;
        } else if seam.distance < boundary.second.distance {
            boundary.second = seam;
        }
    }
    boundary.edge = min(boundary.rim, boundary.first.distance);
    return boundary;
}
