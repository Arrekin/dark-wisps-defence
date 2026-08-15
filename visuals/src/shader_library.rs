//! Registration for shaders that are only reachable through a WGSL `#import`.

use bevy::{prelude::*, shader::Shader};

/// Shaders that exist only to be imported by other shaders. No material points at one, so
/// without a handle held here the asset is dropped and the import stops resolving.
#[derive(Resource, Default)]
pub struct ShaderLibraries(Vec<Handle<Shader>>);

pub trait ShaderLibraryAppExt {
    /// Loads a shader reached only through `#import` and holds it for the lifetime of the app.
    fn register_shader_library(&mut self, path: &'static str) -> &mut Self;
}

impl ShaderLibraryAppExt for App {
    fn register_shader_library(&mut self, path: &'static str) -> &mut Self {
        let handle = self.world().resource::<AssetServer>().load(path);
        // Inserted by whichever plugin registers first rather than by a plugin of its own, so
        // that registering a library imposes no ordering between plugins.
        self.world_mut()
            .get_resource_or_insert_with(ShaderLibraries::default)
            .0
            .push(handle);
        self
    }
}
