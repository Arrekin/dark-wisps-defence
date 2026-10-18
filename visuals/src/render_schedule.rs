use bevy::{
    camera::Hdr,
    ecs::schedule::ScheduleLabel,
    prelude::*,
    render::{camera::CameraRenderGraph, view::Msaa},
};

/// Camera render schedule based on Bevy's `Core2d`, with a banded transparent pass.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BandedCore2d;

/// A 2D camera using [`BandedCore2d`].
/// Requires HDR and disables MSAA because cut pipelines read and write single-sample HDR textures.
#[derive(Component, Default)]
#[require(Camera2d, CameraRenderGraph = CameraRenderGraph::new(BandedCore2d), Hdr, Msaa = Msaa::Off)]
pub struct BandedCamera;
