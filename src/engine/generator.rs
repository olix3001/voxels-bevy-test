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
            render_distance: 16,
            generation_distance: 18,
        }
    }

    pub fn default_with(generator: impl WorldGenerator + 'static) -> Self {
        Self {
            generator: Arc::new(generator),
            render_distance: 16,
            generation_distance: 18,
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

pub struct PerlinHeightmapWorldGenerator {
    pub seed: u32,
    pub scale: f64,
    pub ground_level: i32,
    pub height: f64,
}

impl Default for PerlinHeightmapWorldGenerator {
    fn default() -> Self {
        Self {
            seed: 2138129,
            scale: 64.0,
            ground_level: 0,
            height: 32.0,
        }
    }
}

impl WorldGenerator for PerlinHeightmapWorldGenerator {
    fn generate_chunk(&self, _config: &WorldGeneratorConfig, chunk: &mut Chunk) {
        use noise::{NoiseFn, Perlin};
        let my_noise = Arc::new(Perlin::new(self.seed));

        chunk.generate_with(|chunk_pos, pos| {
            let world_pos = chunk_pos.inner_to_world_position(pos);
            let height = my_noise.get([
                (world_pos.x as f64) / self.scale,
                (world_pos.z as f64) / self.scale,
            ]) * self.height + self.ground_level as f64;
            if world_pos.y < height as f32 {
                Voxel::NonEmpty { is_opaque: true }
            } else {
                Voxel::Empty
            }
        })
    }
}

#[derive(Resource, Debug, PartialEq, Eq, Clone, Copy)]
pub enum GeneratorState {
    Generating,
    Paused,
}

pub struct ChunkGeneratorPlugin;

impl Plugin for ChunkGeneratorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GeneratorState::Generating);
        app.add_systems(Update, (
            update_visible_chunks,
            begin_chunk_generation.after(update_visible_chunks),
            update_generated_chunks,
            unload_invisible_chunks,
            schedule_chunk_meshing,
            apply_meshes,
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
    generator_state: Res<GeneratorState>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    let camera = camera_query.single();
    let camera_position = camera.translation;
    let camera_forward = camera.forward();

    let mut queue = VecDeque::new();

    let current_chunk = ChunkPosition::from_world_position(camera_position);
    let camera_chunk_position = current_chunk.clone();
    queue.push_back((current_chunk, None));

    let mut already_seen: HashSet<ChunkPosition> = HashSet::default();
    already_seen.insert(current_chunk);

    // Add all immediate neighbors to the queue
    for (neighbor, face) in current_chunk.neighbors().iter() {
        queue.push_back((*neighbor, Some(face.opposite())));
        already_seen.insert(*neighbor);
    }

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
            if face.normal().dot(camera_forward) < -0.75 {
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
    if already_seen.len() <= 5 && chunk_data.visible.len() > 5 {
        return;
    }
    chunk_data.visible = already_seen;
}

#[derive(Component)]
pub struct ChunkGenerationTask(pub Task<Chunk>);
/// Generates chunks that are awaiting generation
pub fn begin_chunk_generation(
    mut commands: Commands,
    config: Res<WorldGeneratorConfig>,
    query: Query<(Entity, &AwaitingGeneration)>,
    generator_state: Res<GeneratorState>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

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
    generator_state: Res<GeneratorState>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    for (entity, mut task) in query.iter_mut() {
        if let Some(chunk) = block_on(futures_lite::future::poll_once(&mut task.0)) {
            let chunk_pos = chunk.position;

            let id = commands.entity(entity)
                .remove::<ChunkGenerationTask>()
                .insert(chunk).id();

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
    generator_state: Res<GeneratorState>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    for (entity, chunk) in chunks_query.iter() {
        if !chunk_data.visible.contains(&chunk.position) {
            commands.entity(entity).despawn();
            chunk_data.loaded.remove(&chunk.position);
            chunk_data.awaiting_generation.remove(&chunk.position);
            // NOTE: This is temporary
            chunk_data.meshes.remove(&chunk.position);
        }
    }
}

#[derive(Component)]
pub struct MeshingTask(pub ChunkPosition, pub Task<Option<Mesh>>);
#[derive(Component)]
pub struct EmptyChunkMarker;

impl MeshingTask {
    pub fn new(chunk: &Chunk) -> Self {
        let task_pool = AsyncComputeTaskPool::get();
        let chunk = chunk.clone();
        let position = chunk.position.clone();
        let task = task_pool.spawn(async move {
            let mesh = chunk.build();
            mesh
        });
        Self(position, task)
    }
}

/// Schedules meshing for chunks that have been updated
pub fn schedule_chunk_meshing(
    mut commands: Commands,
    mut query: Query<(Entity, &Chunk), (Without<Handle<Mesh>>, Without<MeshingTask>, Without<EmptyChunkMarker>)>,
    generator_state: Res<GeneratorState>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    for (entity, chunk) in query.iter_mut() {
        let task = MeshingTask::new(chunk);
        commands.entity(entity).try_insert(task);
    } 
}

/// Updates chunks that have finished meshing
pub fn apply_meshes(
    mut commands: Commands,
    mut chunk_data: ResMut<ChunkData>,
    mut query: Query<(Entity, &mut MeshingTask)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    generator_state: Res<GeneratorState>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    for (entity, mut task) in query.iter_mut() {
        if let Some(mesh) = block_on(futures_lite::future::poll_once(&mut task.1)) {
            if mesh.is_none() {
                commands.entity(entity).remove::<MeshingTask>().try_insert(EmptyChunkMarker);
                continue;
            }
            let mesh = mesh.unwrap();
            let mesh_handle = meshes.add(mesh);
            commands.entity(entity).remove::<MeshingTask>().try_insert(PbrBundle {
                mesh: mesh_handle.clone(),
                transform: Transform::from_translation(task.0.as_world_position()),
                material: materials.add(StandardMaterial { base_color: Color::rgb(0.3, 0.85, 0.4), ..Default::default() }),
                ..Default::default()
            });
            chunk_data.meshes.insert(task.0, mesh_handle);
        }
    }
}

/// Debug system to give stats on chunk generation
#[cfg(debug_assertions)]
pub fn show_chunk_generation_debug_info(
    chunk_data: Res<ChunkData>,
    mut contexts: bevy_egui::EguiContexts,
    mut generator_state: ResMut<GeneratorState>,
    mut world_generator_config: ResMut<WorldGeneratorConfig>,
) {
    use bevy_egui::egui;
    egui::Window::new("Chunk Generation").show(&contexts.ctx_mut(), |ui| {
        ui.label(format!("Loaded: {}", chunk_data.loaded.len()));
        ui.label(format!("Awaiting Generation: {}", chunk_data.awaiting_generation.len()));
        ui.label(format!("Visible: {}", chunk_data.visible.len()));
        ui.label(format!("Meshes: {}", chunk_data.meshes.len()));

        ui.separator();

        ui.label(format!("Generator State: {:?}", *generator_state));
        if ui.button("Pause/Resume").clicked() {
            *generator_state = match *generator_state {
                GeneratorState::Generating => GeneratorState::Paused,
                GeneratorState::Paused => GeneratorState::Generating,
            };
        }

        ui.separator();

        ui.label("Chunk Generation Settings");
        ui.add(egui::Slider::new(&mut world_generator_config.render_distance, 1..=64).text("Render Distance"));
        world_generator_config.generation_distance = world_generator_config.render_distance + 2;
        ui.label(format!("Generation Distance: {}", world_generator_config.generation_distance));
    });
}
