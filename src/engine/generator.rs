use std::{collections::VecDeque, sync::Arc};

use bevy::{prelude::*, utils::HashSet, tasks::{Task, AsyncComputeTaskPool, block_on}};

use super::{chunk::{Chunk, ChunkPosition}, voxel::Voxel, ChunkData};

#[derive(Resource, Clone)]
pub struct WorldGeneratorConfig {
    pub generator: Arc<dyn WorldGenerator>,
    pub render_distance: usize,
    /// Chunks at this distance will be generated but not meshed
    pub generation_distance: usize,
}

impl WorldGeneratorConfig {
    pub fn default_flat() -> Self {
        Self {
            generator: Arc::new(FlatWorldGenerator::default()),
            render_distance: 8,
            generation_distance: 10,
        }
    }
}

pub trait WorldGenerator: Send + Sync {
    fn generate_chunk(&self, config: &WorldGeneratorConfig, chunk: &mut Chunk);
}

#[derive(Default)]
pub struct FlatWorldGenerator {
    pub ground_level: i32,
}

impl WorldGenerator for FlatWorldGenerator {
    fn generate_chunk(&self, _config: &WorldGeneratorConfig, chunk: &mut Chunk) {
        chunk.generate_with(|chunk_pos, pos| {
            let world_pos = chunk_pos.inner_to_world_position(pos);
            if world_pos.y < self.ground_level as f32 {
                Voxel::NonEmpty { is_opaque: true }
            } else {
                Voxel::Empty
            }
        })
    }
}

pub struct ChunkGeneratorPlugin;

impl Plugin for ChunkGeneratorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            update_visible_chunks,
            begin_chunk_generation.after(update_visible_chunks),
            update_generated_chunks,
            unload_invisible_chunks,
        ));

        #[cfg(debug_assertions)]
        app.add_systems(Update, show_chunk_generation_debug_info);
    }
}

#[derive(Component)]
pub struct AwaitingGeneration {
    pub chunk_pos: ChunkPosition,
}

/// Updates visible chunks based on the player's position.
pub fn update_visible_chunks(
    mut commands: Commands,
    mut chunk_data: ResMut<ChunkData>,
    config: Res<WorldGeneratorConfig>,
    camera_query: Query<&Transform, With<Camera>>,
    chunks_query: Query<(Entity, &Chunk)>,
) {
    let camera = camera_query.single();
    let camera_position = camera.translation;
    let camera_forward = camera.forward();

    let mut queue = VecDeque::new();

    let current_chunk = ChunkPosition::from_world_position(camera_position);
    let camera_chunk_position = current_chunk.clone();
    queue.push_back((current_chunk, None));

    let mut already_seen: HashSet<ChunkPosition> = HashSet::default();
    already_seen.insert(current_chunk);

    while let Some((chunk_pos, from_face)) = queue.pop_front() {
        // Get chunk if it exists
        let current_chunk = chunk_data.loaded.get(&chunk_pos);
        if current_chunk.is_none() {
            // If chunk does not exist, queue it for generation
            if !chunk_data.awaiting_generation.contains_key(&chunk_pos) {
                let id = commands.spawn((AwaitingGeneration { chunk_pos },)).id();
                chunk_data.awaiting_generation.insert(chunk_pos, id);
            }
            continue;
        }

        let current_chunk = chunks_query.get(*current_chunk.unwrap());
        if current_chunk.is_err() {
            continue;
        }

        let current_chunk = current_chunk.unwrap();

        // Queue all neighbors
        for (neighbor, face) in chunk_pos.neighbors().iter() {
            // Filter 0: Don't go back
            if Some(*face) == from_face {
                continue;
            }

            // Filter 1: Check if we are going in the correct direction
            if face.normal().dot(camera_forward) < -0.5 {
                continue;
            } 

            // Filter 2: Check if we can see the chunk using visibility mask
            if current_chunk.1.is_face_opaque(*face) {
                continue;
            }

            // Filter 3: Check if we are within generation distance
            if camera_chunk_position.distance_to(&neighbor) > config.generation_distance as f32 {
                continue;
            }

            // Last filter: Ensure we have not already seen this chunk
            if already_seen.contains(neighbor) {
                continue;
            }

            // If we pass all filters, queue the chunk
            queue.push_back((*neighbor, Some(face.opposite())));
            already_seen.insert(*neighbor);
        }
    }

    // Update visible chunks
    chunk_data.visible = already_seen;
}

#[derive(Component)]
pub struct ChunkGenerationTask(pub Task<Chunk>);
/// Generates chunks that are awaiting generation
pub fn begin_chunk_generation(
    mut commands: Commands,
    config: Res<WorldGeneratorConfig>,
    query: Query<(Entity, &AwaitingGeneration)>,
) {
    let task_pool = AsyncComputeTaskPool::get();

    for (entity, awaiting_generation) in query.iter() {
        let chunk_pos = awaiting_generation.chunk_pos;
        let chunk = Chunk::new(chunk_pos);
        let config = config.clone();
        let task = task_pool.spawn(async move {
            let mut clone = chunk.clone();
            config.generator.generate_chunk(&config, &mut clone);
            clone.recalculate_visibility_mask();
            clone
        });
        commands.entity(entity)
            .insert(ChunkGenerationTask(task))
            .remove::<AwaitingGeneration>();
    }
}

/// Updates chunks that have finished generating
pub fn update_generated_chunks(
    mut commands: Commands,
    mut chunk_data: ResMut<ChunkData>,
    mut query: Query<(Entity, &mut ChunkGenerationTask)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, mut task) in query.iter_mut() {
        if let Some(chunk) = block_on(futures_lite::future::poll_once(&mut task.0)) {
            let chunk_pos = chunk.position;

            let id = commands.entity(entity)
                .remove::<ChunkGenerationTask>()
                .insert(chunk)
                .insert(PbrBundle {
                    mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
                    transform: Transform::from_translation(chunk_pos.as_world_position()),
                    ..Default::default()
                }).id();

            chunk_data.loaded.insert(chunk_pos, id);
            chunk_data.awaiting_generation.remove(&chunk_pos);
        }
    }
}

/// Removes chunks that should no longer be loaded
pub fn unload_invisible_chunks(
    mut commands: Commands,
    mut chunk_data: ResMut<ChunkData>,
    chunks_query: Query<(Entity, &Chunk)>,
) {
    for (entity, chunk) in chunks_query.iter() {
        if !chunk_data.visible.contains(&chunk.position) {
            commands.entity(entity).despawn();
            chunk_data.loaded.remove(&chunk.position);
        }
    }
}

/// Debug system to give stats on chunk generation
#[cfg(debug_assertions)]
pub fn show_chunk_generation_debug_info(
    chunk_data: Res<ChunkData>,
    mut contexts: bevy_egui::EguiContexts,
) {
    use bevy_egui::egui;
    egui::Window::new("Chunk Generation").show(&contexts.ctx_mut(), |ui| {
        ui.label(format!("Loaded: {}", chunk_data.loaded.len()));
        ui.label(format!("Awaiting Generation: {}", chunk_data.awaiting_generation.len()));
        ui.label(format!("Visible: {}", chunk_data.visible.len()));
    });
}
