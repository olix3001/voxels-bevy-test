use std::hash::Hash;

use bevy::{prelude::*, render::{mesh::{VertexAttributeValues, Indices}, render_resource::PrimitiveTopology, primitives::Aabb}, utils::HashMap};
use crate::{util::{octree::VoxelOctree, Face}, voxel::{Voxel, OptionalVoxel}};

pub mod generator;

pub const CHUNK_SIZE: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkPos(pub Vec3);
impl Into<Vec3> for ChunkPos {
    fn into(self) -> Vec3 {
        self.0 * CHUNK_SIZE as f32
    }
}
impl From<Vec3> for ChunkPos {
    fn from(v: Vec3) -> Self {
        ChunkPos((v / CHUNK_SIZE as f32).floor())
    }
}
impl Eq for ChunkPos {}

#[derive(Component, Clone)]
pub struct Chunk {
    /// Position of the chunk in world space (in chunk units)
    position: ChunkPos,
    /// Octree containing voxels
    octree: VoxelOctree<Voxel>,
    /// Bitmask of which chunk faces are opaque
    /// This goes in the order of top, bottom, left, right, front, back.
    /// 1 means opaque, 0 means transparent.
    opaque_faces: u8,
}

impl Hash for ChunkPos {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.x.to_bits().hash(state);
        self.0.y.to_bits().hash(state);
        self.0.z.to_bits().hash(state);
    }
}

impl Chunk {

    pub fn new(position: Vec3) -> Self {
        Chunk {
            position: ChunkPos(position),
            octree: VoxelOctree::new(CHUNK_SIZE).unwrap(),
            opaque_faces: 0,
        }
    }
    pub fn at(position: ChunkPos) -> Self {
        Chunk {
            position,
            octree: VoxelOctree::new(CHUNK_SIZE).unwrap(),
            opaque_faces: 0,
        }
    }

    pub fn insert(&mut self, position: Vec3, voxel: Voxel) {
        self.octree.insert(position, voxel);
    }

    pub fn get(&self, position: Vec3) -> Option<Voxel> {
        self.octree.get(position)
    }

    /// Recalculates which faces are fully opaque for later use in culling.
    pub fn recalculate_opaque_faces(&mut self) {
        let mut opaque_faces = 0b00000000;
        // Top and bottom faces
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let top = self.get(Vec3::new(x as f32, CHUNK_SIZE as f32 - 1.0, z as f32));
                let bottom = self.get(Vec3::new(x as f32, 0.0, z as f32));

                if top.is_none() || !top.unwrap().is_opaque {
                    opaque_faces |= 1 << 0;
                }
                if bottom.is_none() || !bottom.unwrap().is_opaque {
                    opaque_faces |= 1 << 1;
                }
            }
        }

        // Left and right faces
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let left = self.get(Vec3::new(0.0, y as f32, z as f32));
                let right = self.get(Vec3::new(CHUNK_SIZE as f32 - 1.0, y as f32, z as f32));

                if left.is_none() || !left.unwrap().is_opaque {
                    opaque_faces |= 1 << 2;
                }
                if right.is_none() || !right.unwrap().is_opaque {
                    opaque_faces |= 1 << 3;
                }
            }
        }

        // Front and back faces
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                let front = self.get(Vec3::new(x as f32, y as f32, 0.0));
                let back = self.get(Vec3::new(x as f32, y as f32, CHUNK_SIZE as f32 - 1.0));

                if front.is_none() || !front.unwrap().is_opaque {
                    opaque_faces |= 1 << 4;
                }
                if back.is_none() || !back.unwrap().is_opaque {
                    opaque_faces |= 1 << 5;
                }
            }
        }

        // Flip the bits so 1 means opaque and 0 means transparent
        self.opaque_faces = opaque_faces ^ 0b11111111;
    }

    /// Check whether the given face is opaque.
    pub fn is_face_opaque(&self, face: Face) -> bool {
        (self.opaque_faces >> face.as_num()) & 0b1 == 1
    }

    /// Generates a mesh for the chunk. Detail level of 1 means every voxel will be displayed. 
    /// Detail level of 2 means geometry will be simplified into higher level voxels.
    pub fn generate_mesh(&self, detail: usize) -> Mesh {
        use block_mesh::{ndshape::{RuntimeShape, Shape}, GreedyQuadsBuffer, greedy_quads, RIGHT_HANDED_Y_UP_CONFIG};

        if detail != 1 { panic!("detail != 1 not implemented yet") }
        
        let chunk_size_detail = CHUNK_SIZE / detail;
        let shape = RuntimeShape::<u32, 3>::new([chunk_size_detail as u32 + 2; 3]);
        let shrinked_shape = RuntimeShape::<u32, 3>::new([chunk_size_detail as u32; 3]);

        let mut voxels = vec![OptionalVoxel::Empty; shape.size() as usize];
        for i in 0..(chunk_size_detail).pow(3) {
            let [x, y, z] = shrinked_shape.delinearize(i as u32);
            let voxel = self.get(Vec3::new(x as f32, y as f32, z as f32));
            let index = shape.linearize([x as u32 + 1, y as u32 + 1, z as u32 + 1]);
            voxels[index as usize] = OptionalVoxel::from(voxel);
        }

        // for (i, v) in voxels.iter().enumerate() {
        //     if block_mesh::Voxel::get_visibility(v) != block_mesh::VoxelVisibility::Empty {
        //         let [x, y, z] = shape.delinearize(i as u32);
        //         println!("found non-empty voxel at {x}, {y}, {z} - {i}");
        //     }
        // }

        let mut buffer = GreedyQuadsBuffer::new((chunk_size_detail as u32 + 2).pow(3) as usize);
        let faces = RIGHT_HANDED_Y_UP_CONFIG.faces;
        greedy_quads(
            &voxels,
            &shape,
            [0; 3],
            [chunk_size_detail as u32 + 1; 3],
            &faces,
            &mut buffer,
        );

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

        // println!("#quads: {}", buffer.quads.num_quads());

        let mut indices = Vec::with_capacity(buffer.quads.num_quads() * 6);
        let mut positions = Vec::with_capacity(buffer.quads.num_quads() * 4);
        let mut normals = Vec::with_capacity(buffer.quads.num_quads() * 4);
        for (group, face) in buffer.quads.groups.into_iter().zip(faces.into_iter()) {
            for quad in group.into_iter() {
                indices.extend_from_slice(&face.quad_mesh_indices(positions.len() as u32));
                let _positions = face.quad_mesh_positions(&quad, 1.0);
                // Translate positions by one unit to align with padding
                let aligned_positions = _positions.iter().map(|p| [p[0] - 1.0, p[1] - 1.0, p[2] - 1.0]).collect::<Vec<_>>();
                positions.extend_from_slice(&aligned_positions);
                normals.extend_from_slice(&face.quad_mesh_normals());
            }
        }

        mesh.set_indices(Some(Indices::U32(indices)));
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(positions));
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, VertexAttributeValues::Float32x3(normals));

        mesh
    }

    /// Generate chunk only with edges filled
    pub fn outlined() -> Self {
        let mut chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    if x == 0 && y == 0 || x == 0 && z == 0 || y == 0 && z == 0 ||
                    x == CHUNK_SIZE - 1 && y == CHUNK_SIZE - 1 ||
                    x == CHUNK_SIZE - 1 && z == CHUNK_SIZE - 1 ||
                    y == CHUNK_SIZE - 1 && z == CHUNK_SIZE - 1 ||
                    x == 0 && y == CHUNK_SIZE - 1 ||
                    x == 0 && z == CHUNK_SIZE - 1 ||
                    y == 0 && z == CHUNK_SIZE - 1 ||
                    x == CHUNK_SIZE - 1 && y == 0 ||
                    x == CHUNK_SIZE - 1 && z == 0 ||
                    y == CHUNK_SIZE - 1 && z == 0 {
                        chunk.insert(Vec3::new(x as f32, y as f32, z as f32), Voxel::opaque());
                    }
                }
            }
        }
        chunk
    }

    /// Converts inner position to world position
    pub fn inner_to_world_position(&self, position: Vec3) -> Vec3 {
        let converted: Vec3 = self.position.clone().into();
        converted + position
    }

    /// Converts world position to inner position
    pub fn world_to_inner_position(&self, position: Vec3) -> Vec3 {
        let converted: Vec3 = self.position.clone().into();
        position - converted
    }

    /// Gets neighbor position.
    pub fn get_neighbor_position(&self, face: Face) -> ChunkPos {
        let mut position = self.position.0;
        match face {
            Face::Top => position += Vec3::new(0.0, 1.0, 0.0),
            Face::Bottom => position += Vec3::new(0.0, -1.0, 0.0),
            Face::Left => position += Vec3::new(-1.0, 0.0, 0.0),
            Face::Right => position += Vec3::new(1.0, 0.0, 0.0),
            Face::Front => position += Vec3::new(0.0, 0.0, 1.0),
            Face::Back => position += Vec3::new(0.0, 0.0, -1.0),
        }
        ChunkPos(position)
    }

    pub fn get_aabb(&self) -> Aabb {
        let min = self.position.0;
        let max = self.position.0 + Vec3::new(CHUNK_SIZE as f32, CHUNK_SIZE as f32, CHUNK_SIZE as f32);
        Aabb::from_min_max(min, max)
    }
}

#[derive(Resource)]
pub struct ChunksData {
    pub chunks: HashMap<ChunkPos, Entity>
}

impl Default for ChunksData {
    fn default() -> Self {
        ChunksData {
            chunks: HashMap::default()
        }
    }
}

impl ChunksData {
    pub fn get_chunk(&self, position: ChunkPos) -> Option<Entity> {
        self.chunks.get(&position).map(|e| *e)
    }

    pub fn insert_chunk(&mut self, position: ChunkPos, entity: Entity) {
        self.chunks.insert(position, entity);
    }

    /// Get the chunk neighbors of the given chunk.
    /// The order is top, bottom, left, right, front, back.
    pub fn get_neighbors(&self, chunk: &ChunkPos) -> [(Option<Entity>, Face); 6] {
        let mut neighbors = [(None, Face::Top); 6];

        // Top neighbor
        neighbors[0] = (self.get_chunk(ChunkPos::from(chunk.0 + Vec3::new(0.0, 1.0, 0.0))), Face::Top);
        // Bottom neighbor
        neighbors[1] = (self.get_chunk(ChunkPos::from(chunk.0 + Vec3::new(0.0, -1.0, 0.0))), Face::Bottom);
        // Left neighbor
        neighbors[2] = (self.get_chunk(ChunkPos::from(chunk.0 + Vec3::new(-1.0, 0.0, 0.0))), Face::Left);
        // Right neighbor
        neighbors[3] = (self.get_chunk(ChunkPos::from(chunk.0 + Vec3::new(1.0, 0.0, 0.0))), Face::Right);
        // Front neighbor
        neighbors[4] = (self.get_chunk(ChunkPos::from(chunk.0 + Vec3::new(0.0, 0.0, 1.0))), Face::Front);
        // Back neighbor
        neighbors[5] = (self.get_chunk(ChunkPos::from(chunk.0 + Vec3::new(0.0, 0.0, -1.0))), Face::Back);

        neighbors
    }

    /// Get chunk that contains the given position.
    pub fn get_chunk_containing(&self, position: Vec3) -> Option<Entity> {
        let chunk_pos = ChunkPos::from(position);
        self.get_chunk(chunk_pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_creation() {
        let chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(chunk.position, ChunkPos(Vec3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn test_chunk_insert() {
        let mut chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        chunk.insert(Vec3::new(0.0, 0.0, 0.0), Voxel::opaque());
        assert_eq!(chunk.get(Vec3::new(0.0, 0.0, 0.0)), Some(Voxel::opaque()));
    }

    #[test]
    fn test_chunk_get() {
        let mut chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        chunk.insert(Vec3::new(0.0, 0.0, 0.0), Voxel::opaque());
        assert_eq!(chunk.get(Vec3::new(0.0, 0.0, 0.0)), Some(Voxel::opaque()));
    }

    #[test]
    fn test_chunk_get_none() {
        let chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(chunk.get(Vec3::new(0.0, 0.0, 0.0)), None);
    }

    #[test]
    fn test_chunk_opaque_faces() {
        let mut chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        // Fill top face
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.insert(Vec3::new(x as f32, CHUNK_SIZE as f32 - 1.0, z as f32), Voxel::opaque());
            }
        }

        chunk.recalculate_opaque_faces();

        assert!(chunk.is_face_opaque(Face::Top));
        assert!(!chunk.is_face_opaque(Face::Bottom));
    }

    #[test]
    fn test_chunk_pos_eq() {
        let chunk_pos_1 = ChunkPos(Vec3::new(0.0, 0.0, 0.0));
        let chunk_pos_2 = ChunkPos(Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(chunk_pos_1, chunk_pos_2);
    }

    #[test]
    fn test_bottom_face_opaque() {
        let mut chunk = Chunk::new(Vec3::new(0.0, 0.0, 0.0));
        // Fill bottom face
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.insert(Vec3::new(x as f32, 0.0, z as f32), Voxel::opaque());
            }
        }

        chunk.recalculate_opaque_faces();

        assert!(chunk.is_face_opaque(Face::Bottom));
        assert!(!chunk.is_face_opaque(Face::Top));
        assert!(!chunk.is_face_opaque(Face::Left));
    }
}