use bevy::{prelude::*, utils::{HashMap, HashSet}};

use self::{chunk::ChunkPosition, generator::ChunkGeneratorPlugin};

pub mod chunk;
pub mod voxel;
pub mod util;
pub mod generator;

#[derive(Debug, Resource)]
pub struct ChunkData {
    /// Keeps track of chunk meshes when they are generated, updated, and destroyed
    pub meshes: HashMap<ChunkPosition, Handle<Mesh>>,
    /// Keeps track of which chunks are already loaded
    pub loaded: HashMap<ChunkPosition, Entity>,
    /// Keeps track of which chunks are awaiting generation
    pub awaiting_generation: HashMap<ChunkPosition, Entity>,
    /// Visible chunks around the player, these should be loaded and have meshes
    pub visible: HashSet<ChunkPosition>,
}

impl Default for ChunkData {
    fn default() -> Self {
        Self {
            meshes: HashMap::default(),
            loaded: HashMap::default(),
            awaiting_generation: HashMap::default(),
            visible: HashSet::default(),
        }
    }
}

impl ChunkData {
    pub fn forget(&mut self, chunk: ChunkPosition) {
        self.meshes.remove(&chunk);
        self.loaded.remove(&chunk);
        self.awaiting_generation.remove(&chunk);
    } 
}

pub struct ChunkPlugin;

impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(ChunkData::default())
            .insert_resource(generator::WorldGeneratorConfig::default_with(generator::PerlinHeightmapWorldGenerator::default()))
            .add_plugins(ChunkGeneratorPlugin);

        #[cfg(debug_assertions)]
        app.add_plugins(bevy_egui::EguiPlugin);
    }
}