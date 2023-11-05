use std::sync::Arc;

use bevy::{prelude::*, utils::HashMap, tasks::{AsyncComputeTaskPool, Task, block_on}};

use crate::{flycam::prelude::Voxel, util::Face};

use super::{ChunksData, ChunkPos, CHUNK_SIZE, Chunk};

pub trait WorldGenerator: Sync + Send {
    fn get_voxel_at(&self, position: Vec3) -> Option<Voxel>;
}

#[derive(Default, Clone)]
pub struct FlatWorldGenerator {
    height: usize,
}

impl WorldGenerator for FlatWorldGenerator {
    fn get_voxel_at(&self, position: Vec3) -> Option<Voxel> {
        if position.y as usize <= self.height {
            Some(Voxel::opaque())
        } else {
            None
        }
    }
}

pub struct ChunkGeneratorPlugin {
    pub world_generator: Arc<dyn WorldGenerator>,
}

#[derive(Resource)]
pub struct WorldGeneratorResource {
    world_generator: Arc<dyn WorldGenerator>,
}

impl WorldGeneratorResource {
    pub fn generate_chunk(&self, chunk_position: ChunkPos) -> Chunk {
        let mut chunk = Chunk::at(chunk_position);

        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let pos = chunk.inner_to_world_position(Vec3::new(x as f32, y as f32, z as f32));
                    if let Some(voxel) = self.world_generator.get_voxel_at(pos) {
                        chunk.insert(Vec3::new(x as f32, y as f32, z as f32), voxel);
                    }
                }
            }
        }

        chunk.recalculate_opaque_faces();
        chunk
    }
}

impl ChunkGeneratorPlugin {
    pub fn with_flat_world_generator(height: usize) -> Self {
        Self {
            world_generator: Arc::new(FlatWorldGenerator { height }),
        }
    }
}

impl Plugin for ChunkGeneratorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldGeneratorResource {
            world_generator: self.world_generator.clone(),
        });
        app.add_event::<RemoveHiddenChunksEvent>();
        app.add_event::<RequestMeshEvent>();
        app.insert_resource(ChunksData::default());
        app.add_systems(Update, (
            update_chunks,
            generate_awaiting_meshes.before(update_chunks),
            add_meshes_to_chunks,
            remove_hidden_chunks,
        ));
    }
}

/// Runs voxel culling as described in https://tomcc.github.io/2014/08/31/visibility-2.html.
/// This allows to reduce the amount of chunks that need to be rendered.
/// This takes the current player position and chunk data as input and outputs a list of chunks that should be visible, generating new chunks if necessary.
fn cull_chunks(
    player_position: Vec3,
    player_direction: Vec3,
    chunks_data: &mut ResMut<ChunksData>,
    mut commands: &mut Commands,
    world_generator: &Res<WorldGeneratorResource>,
    chunks_q: &Query<(Entity, &Chunk)>,
) -> Vec<(ChunkPos, Entity)> {
    // First, get the chunk position of the player.
    let player_chunk_position = ChunkPos::from(player_position);

    // Ensure that the player chunk is loaded.
    let player_chunk_entity = ensure_chunk_loaded(player_chunk_position.clone(), chunks_data, &mut commands, &world_generator, &chunks_q);

    // Create queue of chunks to check.
    let mut chunks_to_check = Vec::new();
    chunks_to_check.push((player_chunk_entity.0, player_chunk_entity.1, (false, Face::Top)));
    
    // Create list of chunks that should be visible.
    let mut visible_chunks = Vec::new();
    visible_chunks.push((player_chunk_position.clone(), player_chunk_entity.0));

    // List of visible chunks for fast lookup.
    let mut visible_chunks_lookup = HashMap::new();

    // Iterate over all chunks to check.
    while let Some((_chunk_entity, chunk, came_from)) = chunks_to_check.pop() {

        // Filter adjacent chunks.
        let adjacent_chunks = chunks_data.get_neighbors(&chunk.position);
        for (_adjacent_chunk, adj_chunk_face) in adjacent_chunks.iter() {
            // Don't revisit the chunk we came from.
            if came_from.0 && came_from.1 == *adj_chunk_face {
                continue;
            }

            // Filter 1: Check if we are going in the right direction.
            let face_dir = adj_chunk_face.normal();
            let dot = face_dir.dot(player_direction);
            if dot < -0.5 {
                continue;
            }

            // Filter 2: Check if the face is fully opaque.
            if chunk.is_face_opaque(*adj_chunk_face) {
                continue; // We can't see through this face, so we don't need to check the adjacent chunk.
            }

            // Ensure that the adjacent chunk is loaded.
            let adj_position = chunk.get_neighbor_position(*adj_chunk_face);
            if visible_chunks_lookup.get(&adj_position).is_some() {
                continue; // We already visited this chunk.
            }
            let adj_chunk_entity = ensure_chunk_loaded(adj_position.clone(), chunks_data, commands, &world_generator, &chunks_q);

            // Pre-filter: Check distance
            let distance = (chunk.inner_to_world_position(Vec3::new(0.0, 0.0, 0.0)) - player_position).length();
            if distance > 100.0 {
                continue;
            }

            // Add adjacent chunk to visible chunks.
            visible_chunks_lookup.insert(adj_position.clone(), adj_chunk_entity.0);
            visible_chunks.push((adj_position.clone(), adj_chunk_entity.0));

            // Add adjacent chunk to chunks to check.
            chunks_to_check.push((adj_chunk_entity.0, adj_chunk_entity.1, (true, adj_chunk_face.opposite())));
        }
    }

    visible_chunks
}

/// Ensure that the chunk at the given position is loaded.
/// If the chunk is not loaded, it will be generated / loaded.
fn ensure_chunk_loaded<'a>(
    chunk_position: ChunkPos,
    chunks_data: &mut ResMut<ChunksData>,
    commands: &mut Commands,
    world_generator: &Res<WorldGeneratorResource>,
    chunks_q: &Query<(Entity, &'a Chunk)>,
) -> (Entity, Chunk) {
    // Check if the chunk is already loaded.
    if chunks_data.chunks.contains_key(&chunk_position) {
        // Chunk is already loaded, return it.
        let chunk = chunks_q.get(chunks_data.chunks[&chunk_position]);
        if let Ok(chunk) = chunk {
            return (chunks_data.chunks[&chunk_position], chunk.1.clone());
        }
    }

    // Generate the chunk.
    let chunk = world_generator.generate_chunk(chunk_position.clone());
    let chunk_clone = chunk.clone();

    // Create chunk entity (without mesh).
    let chunk_entity = commands.spawn(
        (
            chunk,
            AwaitingMesh
        )
    );

    // Add chunk to chunks data.
    chunks_data.insert_chunk(chunk_position, chunk_entity.id());
    (chunk_entity.id(), chunk_clone)
}

#[derive(Component)]
pub struct AwaitingMesh;

#[derive(Event)]
pub struct RemoveHiddenChunksEvent {
    pub visible_chunks: Arc<Vec<(ChunkPos, Entity)>>,
}

#[derive(Event)]
pub struct RequestMeshEvent {
    pub chunk_entity: Entity,
}

/// System for updating the chunks that should be visible.
pub fn update_chunks(
    camera: Query<(&Transform, &GlobalTransform), With<Camera>>,
    mut commands: Commands,
    mut chunks_data: ResMut<ChunksData>,
    world_generator: Res<WorldGeneratorResource>,
    query: Query<(Entity, &Chunk)>,
    mut event_writer: EventWriter<RemoveHiddenChunksEvent>,
    mut request_mesh_writer: EventWriter<RequestMeshEvent>,
    with_mesh_query: Query<Entity, With<Handle<Mesh>>>,
    awaiting_mesh_query: Query<Entity, With<AwaitingChunkMesh>>,
) {
    let camera_transform = camera.single().0;
    let camera_position = camera_transform.translation;
    let camera_direction = camera_transform.forward();

    // Cull chunks.
    let visible_chunks = Arc::new(cull_chunks(camera_position, camera_direction, &mut chunks_data, &mut commands, &world_generator, &query));

    // println!("Visible chunks: {}", visible_chunks.len());
    // Add event to remove hidden chunks.
    event_writer.send(RemoveHiddenChunksEvent {
        visible_chunks: visible_chunks.clone(),
    });

    // Add AwaitingMesh component to all visible chunks that don't have a mesh yet.
    for (_chunk_pos, chunk_entity) in visible_chunks.iter() {
        if with_mesh_query.get(*chunk_entity).is_err() {
            if awaiting_mesh_query.get(*chunk_entity).is_err() {
                request_mesh_writer.send(RequestMeshEvent {
                    chunk_entity: *chunk_entity,
                });
            }
        }
    }
}

#[derive(Component)]
pub struct AwaitingChunkMesh(pub Task<Mesh>);

/// System for generating meshes for chunks.
pub fn generate_awaiting_meshes(
    mut event_reader: EventReader<RequestMeshEvent>,
    mut commands: Commands,
    chunks: Query<&Chunk, With<AwaitingMesh>>
) {
    let task_pool = AsyncComputeTaskPool::get();
    for event in event_reader.read() {
        // Spawn task
        let my_chunk = chunks.get(event.chunk_entity);
        if let Err(_) = my_chunk {
            continue;
        }
        let my_chunk = my_chunk.unwrap().clone();
        let mesh_task = task_pool.spawn(async move {
            let mesh = my_chunk.generate_mesh(1);
            mesh 
        });

        // Add AwaitingChunkMesh component to chunk.
        commands.entity(event.chunk_entity).try_insert(AwaitingChunkMesh(mesh_task));
    }
}

/// System for adding meshes to chunks.
pub fn add_meshes_to_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunks: Query<(Entity, &Chunk, &mut AwaitingChunkMesh)>,
) {
    for (chunk_entity, chunk, mut mesh_data) in chunks.iter_mut() {
        if let Some(mesh) = block_on(futures_lite::future::poll_once(&mut mesh_data.0)) {
            commands.entity(chunk_entity).remove::<AwaitingChunkMesh>().remove::<AwaitingMesh>();
            commands.entity(chunk_entity).try_insert(PbrBundle {
                mesh: meshes.add(mesh),
                material: materials.add(Color::rgba(0.8, 0.3, 0.4, 0.5).into()),
                transform: Transform::from_translation(chunk.inner_to_world_position(Vec3::ZERO)),
                ..Default::default()
            });
        }
    }
}

/// System for removing chunks that are not visible anymore.
pub fn remove_hidden_chunks(
    mut commands: Commands,
    mut events: EventReader<RemoveHiddenChunksEvent>,
    chunk_query: Query<Entity, With<Chunk>>,
) {
    for event in events.read() {
        for chunk_entity in chunk_query.iter() {
            if event.visible_chunks.iter().find(|(_, ent)| *ent == chunk_entity).is_none() {
                commands.entity(chunk_entity).despawn_recursive();
            }
        } 
    }
}