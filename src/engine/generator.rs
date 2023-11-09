use std::{collections::VecDeque, sync::Arc};

use bevy::{prelude::*, utils::HashSet, tasks::{Task, AsyncComputeTaskPool, block_on}, core::FrameCount, render::primitives::Frustum};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};

use super::{chunk::{Chunk, ChunkPosition}, voxel::Voxel, ChunkData, util::intersects_frustum};

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
        
        app.add_systems(PostUpdate, garbage_collect_chunks);

        #[cfg(debug_assertions)]
        app.add_systems(Update, show_chunk_generation_debug_info);
        #[cfg(debug_assertions)]
        app.insert_resource(ChunkGenerationStatsDebugTimeseries::new(100));
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
    camera_query: Query<(&Transform, &Projection), With<Camera>>,
    chunks_query: Query<(Entity, &Chunk)>,
    generator_state: Res<GeneratorState>,
    unmeshed_chunks_query: Query<Entity, (Without<Handle<Mesh>>, With<Chunk>)>,
    frustum: Query<&Frustum, With<Camera>>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    let camera = camera_query.single();
    let camera_position = camera.0.translation;
    let camera_forward = camera.0.forward();

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

    let frustum = frustum.single();

    while let Some((chunk_pos, from_face)) = queue.pop_front() {
        // Get chunk if it exists
        let current_chunk = chunk_data.loaded.get(&chunk_pos).map(|entity| *entity);
        if current_chunk.is_none() {
            // If chunk does not exist, queue it for generation
            if !chunk_data.awaiting_generation.contains_key(&chunk_pos) {
                let id = commands.spawn((AwaitingGeneration { chunk_pos },)).id();
                chunk_data.awaiting_generation.insert(chunk_pos, id);
            }
            // Exception: If chunk is close enough to the player, treat it as if it is loaded
            if camera_chunk_position.distance_to(&chunk_pos) > 2.5 {
                continue;
            }
        } else {
            // If chunk is loaded, check whether we have meshed it yet
            if chunk_data.meshes.contains_key(&chunk_pos) {
                // If chunk was not visible before, add mesh we already have
                if let Ok(entity) = unmeshed_chunks_query.get(current_chunk.unwrap()) {
                    let mesh_handle = chunk_data.meshes.get(&chunk_pos);
                    commands.entity(entity).try_insert(mesh_handle.unwrap().clone());
                }
            }
        }

        let current_chunk = if current_chunk.is_some() {
            let current_chunk = chunks_query.get(current_chunk.unwrap());
            if current_chunk.is_err() {
                continue;
            }

            Some(current_chunk.unwrap())
        } else {
            None
        };

        // Queue all neighbors
        for (neighbor, face) in chunk_pos.neighbors().iter() {
            // Filter 0: Don't go back
            if Some(*face) == from_face {
                continue;
            }

            // Filter 1: Check if we are going in the correct direction
            let view_vector = (face.face_center_in_chunk(&chunk_pos) - camera_position).normalize();
            if camera_forward.dot(view_vector) < 0.0 {
                continue;
            }

            // Filter 2: Check if we can see the chunk using visibility mask
            if current_chunk.is_some() && current_chunk.unwrap().1.is_face_opaque(*face) {
                continue;
            }

            // Filter 3: Check if we are within generation distance
            if camera_chunk_position.distance_to(&neighbor) > config.generation_distance as f32 {
                continue;
            }

            // Filter 4: Ensure we have not already seen this chunk
            if already_seen.contains(neighbor) {
                continue;
            }

            // Filter 5: Check if chunk is in frustum
            if !intersects_frustum(neighbor, &frustum) {
                continue;
            }

            // If we pass all filters, queue the chunk
            queue.push_back((*neighbor, Some(face.opposite())));
            already_seen.insert(*neighbor);
        }
    }

    // Yup, this number is not arbitrary at all
    if chunk_data.visible.len() > 7 && already_seen.len() == 7 {
        return; // TODO: This is a hacky fix, find a better way to do this
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
            // commands.entity(entity).despawn();
            commands.entity(entity).remove::<Handle<Mesh>>();
            // chunk_data.loaded.remove(&chunk.position);
            chunk_data.awaiting_generation.remove(&chunk.position);
            // NOTE: This is temporary
            // chunk_data.meshes.remove(&chunk.position);
        }
    }
}

pub enum MeshState {
    /// A mesh that has been loaded from memory
    Loaded(Handle<Mesh>),
    /// A mesh that is currently being loaded
    Loading(Task<Option<Mesh>>),
}
#[derive(Component)]
pub struct MeshingTask(pub ChunkPosition, pub MeshState);
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
        Self(position, MeshState::Loading(task))
    }
}

/// Schedules meshing for chunks that have been updated
pub fn schedule_chunk_meshing(
    mut commands: Commands,
    mut query: Query<(Entity, &Chunk), (Without<Handle<Mesh>>, Without<MeshingTask>, Without<EmptyChunkMarker>)>,
    generator_state: Res<GeneratorState>,
    chunk_data: Res<ChunkData>,
) {
    if *generator_state == GeneratorState::Paused {
        return;
    }

    for (entity, chunk) in query.iter_mut() {
        // If chunk is meshed, skip it
        if chunk_data.meshes.contains_key(&chunk.position) {
            continue;
        }
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
        let mesh_handle = match &mut task.1 {
            MeshState::Loaded(ref handle) => Some(handle.clone()),
            MeshState::Loading(ref mut mesh_task) => {
                if let Some(mesh) = block_on(futures_lite::future::poll_once(mesh_task)) {
                    if mesh.is_none() {
                        commands.entity(entity).remove::<MeshingTask>().try_insert(EmptyChunkMarker);
                        continue;
                    }
                    let mesh = mesh.unwrap();
                    let mesh_handle = meshes.add(mesh);
                    Some(mesh_handle)
                } else { None }
            },
        };
        if let Some(mesh_handle) = mesh_handle {
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

/// Garbage collector :D
/// Removes chunks and meshes that are too far away or that have other reasons to be removed
/// This runs every few seconds or if there is enough time left in the frame
pub fn garbage_collect_chunks(
    mut commands: Commands,
    mut chunk_data: ResMut<ChunkData>,
    chunks_query: Query<(Entity, &Chunk)>,
    worldgen_config: Res<WorldGeneratorConfig>,
    time: Res<Time>,
    frame_count: Res<FrameCount>,
    camera: Query<&Transform, With<Camera>>,
) {
    let is_enough_time_left = time.delta_seconds_f64() < 1.0 / 30.0;
    let is_time_to_collect = frame_count.0 % 60 == 0; // Should force garbage collection every second (60 frames)
    let should_force_collect = frame_count.0 % 600 == 0; // Should force garbage collection every 10 seconds (600 frames)
    if !should_force_collect {
        if !is_enough_time_left && !is_time_to_collect {
            return;
        }
    }

    let camera_position = camera.single().translation;

    for (entity, chunk) in chunks_query.iter() {
        if chunk_data.visible.contains(&chunk.position) {
            continue;
        }
        if chunk.position.distance_to(&ChunkPosition::from_world_position(camera_position)) > worldgen_config.generation_distance as f32 {
            commands.entity(entity).despawn_recursive();
            chunk_data.forget(chunk.position);
        }
    }
}

/// Debug resource to keep track of chunk generation stats
#[cfg(debug_assertions)]
#[derive(Resource)]
pub struct ChunkGenerationStatsDebugTimeseries {
    capacity: usize,
    pub loaded: Vec<[f64; 2]>,
    pub awaiting_generation: Vec<[f64; 2]>,
    pub visible: Vec<[f64; 2]>,
    pub meshes: Vec<[f64; 2]>,
}

#[cfg(debug_assertions)]
impl ChunkGenerationStatsDebugTimeseries {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            loaded: Vec::with_capacity(capacity),
            awaiting_generation: Vec::with_capacity(capacity),
            visible: Vec::with_capacity(capacity),
            meshes: Vec::with_capacity(capacity),
        }
    }

    pub fn add(&mut self, timestamp: f64, loaded: f64, awaiting_generation: f64, visible: f64, meshes: f64) {
        if self.loaded.len() >= self.capacity {
            self.loaded.remove(0);
            self.awaiting_generation.remove(0);
            self.visible.remove(0);
            self.meshes.remove(0);
        }
        self.loaded.push([timestamp, loaded]);
        self.awaiting_generation.push([timestamp, awaiting_generation]);
        self.visible.push([timestamp, visible]);
        self.meshes.push([timestamp, meshes]);
    }

    pub fn get_series<'a>(&'a self) -> (&'a [[f64; 2]], &'a [[f64; 2]], &'a [[f64; 2]], &'a [[f64; 2]]) {
        (&self.loaded, &self.awaiting_generation, &self.visible, &self.meshes)
    }
}

/// Debug system to give stats on chunk generation
#[cfg(debug_assertions)]
pub fn show_chunk_generation_debug_info(
    mut chunk_data: ResMut<ChunkData>,
    mut commands: Commands,
    mut contexts: bevy_egui::EguiContexts,
    mut generator_state: ResMut<GeneratorState>,
    mut world_generator_config: ResMut<WorldGeneratorConfig>,
    mut chunk_generation_series: ResMut<ChunkGenerationStatsDebugTimeseries>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time>,
    camera: Query<&Transform, With<Camera>>,
) {
    use bevy_egui::egui;
    egui::Window::new("Chunk Generation").show(&contexts.ctx_mut(), |ui| {
        // Plot of loaded chunks, awaiting generation chunks, visible chunks, and meshes
        let loaded_chunks = chunk_data.loaded.len();
        let awaiting_generation_chunks = chunk_data.awaiting_generation.len();
        let visible_chunks = chunk_data.visible.len();
        let meshes = chunk_data.meshes.len();

        let timestamp = time.elapsed_seconds_f64();
        chunk_generation_series.add(
            timestamp,
            loaded_chunks as f64,
            awaiting_generation_chunks as f64,
            visible_chunks as f64,
            meshes as f64
        );

        let plot = egui_plot::Plot::new("Chunk Generation Stats")
            .legend(egui_plot::Legend::default()
                .position(egui_plot::Corner::LeftBottom)
            )
            .view_aspect(2.0)
            .height(200.0)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show_axes(true)
            .show_grid(true)
            .set_margin_fraction(bevy_egui::egui::Vec2::new(0.05, 0.22));

        plot.show(ui, |plot_ui| {
            let (loaded, awaiting_generation, visible, meshes) = chunk_generation_series.get_series();
            plot_ui.line(
                egui_plot::Line::new(loaded.to_vec())
                    .color(egui::Color32::from_rgb(0, 255, 0))
                    .name("Loaded Chunks")
            );
            plot_ui.line(
                egui_plot::Line::new(awaiting_generation.to_vec())
                    .color(egui::Color32::from_rgb(255, 0, 0))
                    .name("Awaiting Generation Chunks")
            );
            plot_ui.line(
                egui_plot::Line::new(visible.to_vec())
                    .color(egui::Color32::from_rgb(0, 0, 255))
                    .name("Visible Chunks")
            );
            plot_ui.line(
                egui_plot::Line::new(meshes.to_vec())
                    .color(egui::Color32::from_rgb(255, 255, 0))
                    .name("Meshes")
            );
        });
        ui.label(format!(
            "Average FPS: {:.02}",
            diagnostics
                .get(FrameTimeDiagnosticsPlugin::FPS)
                .unwrap()
                .average()
                .unwrap_or_default()
        ));

        ui.label(format!("Player Position: {:?}", camera.single().translation));
        ui.label(format!("Player forward: {:?}", camera.single().forward()));

        ui.separator();

        ui.label(format!("Generator State: {:?}", *generator_state));
        if ui.button("Pause/Resume").clicked() {
            *generator_state = match *generator_state {
                GeneratorState::Generating => GeneratorState::Paused,
                GeneratorState::Paused => GeneratorState::Generating,
            };
        }

        ui.separator();

        ui.label("Clear Data");
        ui.horizontal(|ui| {
            if ui.button("Meshes").clicked() {
                for (_, entity) in chunk_data.loaded.iter() {
                    commands.entity(*entity).remove::<Handle<Mesh>>();
                }
                chunk_data.meshes.clear();
            }
            if ui.button("All").clicked() {
                chunk_data.meshes.clear();
                for (_, entity) in chunk_data.loaded.drain() {
                    commands.entity(entity).despawn_recursive();
                }
                chunk_data.awaiting_generation.clear();
                chunk_data.visible.clear();
            }
        });

        ui.separator();

        ui.label("Chunk Generation Settings");
        ui.add(egui::Slider::new(&mut world_generator_config.render_distance, 1..=64).text("Render Distance"));
        world_generator_config.generation_distance = world_generator_config.render_distance + 2;
        ui.label(format!("Generation Distance: {}", world_generator_config.generation_distance));
    });
}
