use bevy::prelude::*;

use crate::{GridVersion, base::BaseGrid};

pub type ForceFieldGrid = BaseGrid<Option<Entity>, GridVersion>;
