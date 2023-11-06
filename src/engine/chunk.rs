use std::sync::{RwLock, Arc, RwLockReadGuard, RwLockWriteGuard};

use bevy::{prelude::{Vec3, Component, Mesh}, render::mesh::VertexAttributeValues};
use block_mesh::{ndshape::ConstShape, GreedyQuadsBuffer, greedy_quads, RIGHT_HANDED_Y_UP_CONFIG};

use super::{voxel::Voxel, util::Face};

pub const CHUNK_SIZE: usize = 16;
pub type ChunkVoxels = Vec<Voxel>;

/// The shape of a chunk with padding of 1 on each side
type ChunkNDShapePadded = block_mesh::ndshape::ConstShape3u32<{ CHUNK_SIZE as u32 + 2 }, { CHUNK_SIZE as u32 + 2 }, { CHUNK_SIZE as u32 + 2 }>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkPosition {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn from_world_position(pos: Vec3) -> Self {
        Self {
            x: pos.x as i32 / CHUNK_SIZE as i32,
            y: pos.y as i32 / CHUNK_SIZE as i32,
            z: pos.z as i32 / CHUNK_SIZE as i32,
        }
    }

    pub fn as_world_position(&self) -> Vec3 {
        Vec3::new(
            self.x as f32 * CHUNK_SIZE as f32,
            self.y as f32 * CHUNK_SIZE as f32,
            self.z as f32 * CHUNK_SIZE as f32,
        )
    }

    /// Converts a position relative to the chunk to a position in the world.
    pub fn inner_to_world_position(&self, pos: Vec3) -> Vec3 {
        Vec3::new(
            self.x as f32 * CHUNK_SIZE as f32 + pos.x,
            self.y as f32 * CHUNK_SIZE as f32 + pos.y,
            self.z as f32 * CHUNK_SIZE as f32 + pos.z,
        )
    }

    /// Converts a position in the world to a position relative to the chunk.
    pub fn world_to_inner_position(&self, pos: Vec3) -> Vec3 {
        Vec3::new(
            pos.x - self.x as f32 * CHUNK_SIZE as f32,
            pos.y - self.y as f32 * CHUNK_SIZE as f32,
            pos.z - self.z as f32 * CHUNK_SIZE as f32,
        )
    }

    pub fn neighbors(&self) -> [(ChunkPosition, Face); 6] {
        [
            (ChunkPosition::new(self.x - 1, self.y, self.z), Face::Left),
            (ChunkPosition::new(self.x + 1, self.y, self.z), Face::Right),
            (ChunkPosition::new(self.x, self.y - 1, self.z), Face::Bottom),
            (ChunkPosition::new(self.x, self.y + 1, self.z), Face::Top),
            (ChunkPosition::new(self.x, self.y, self.z - 1), Face::Back),
            (ChunkPosition::new(self.x, self.y, self.z + 1), Face::Front),
        ]
    }

    pub fn distance_to(&self, other: &ChunkPosition) -> f32 {
        let dx = (self.x - other.x) as f32;
        let dy = (self.y - other.y) as f32;
        let dz = (self.z - other.z) as f32;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

#[derive(Debug, Clone, Component)]
pub struct Chunk {
    /// The voxel data for this chunk
    data: Arc<RwLock<ChunkVoxels>>,
    /// The position of this chunk
    pub position: ChunkPosition,
    /// The visibility mask for this chunk
    /// This goes in order of the faces of a cube (left, right, bottom, top, back, front)
    /// 1 means that the face is opaque, 0 means that the face is non fully opaque
    pub visibility_mask: u8,
}

impl Chunk {
    pub fn new(position: ChunkPosition) -> Self {
        Self {
            data: Arc::new(RwLock::new(vec![Voxel::default(); CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE])),
            position,
            visibility_mask: 0b000000,
        }
    }

    pub fn get(&self, pos: Vec3) -> Voxel {
        let (x, y, z) = (pos.x as usize, pos.y as usize, pos.z as usize);
        self.data.read().unwrap().get(Chunk::linearize_position(x, y, z)).unwrap().clone()
    }

    pub fn set(&mut self, pos: Vec3, voxel: Voxel) {
        let (x, y, z) = (pos.x as usize, pos.y as usize, pos.z as usize);
        self.data.write().unwrap()[Chunk::linearize_position(x, y, z)] = voxel;
    }

    pub fn reader(&self) -> ChunkDataReader {
        ChunkDataReader {
            data: self.data.read().unwrap()
        }
    }

    pub fn writer(&self) -> ChunkDataWriter {
        ChunkDataWriter {
            data: self.data.write().unwrap()
        }
    }

    pub fn linearize_position(x: usize, y: usize, z: usize) -> usize {
        x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    }

    pub fn delinearize_position(index: usize) -> (usize, usize, usize) {
        let x = index % CHUNK_SIZE;
        let y = (index / CHUNK_SIZE) % CHUNK_SIZE;
        let z = index / CHUNK_SIZE / CHUNK_SIZE;
        (x, y, z)
    }

    pub fn recalculate_visibility_mask(&mut self) {
        let reader = self.reader();
        let mut mask = 0b000000;

        // Left and right
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let left = reader.get(0, y, z);
                let right = reader.get(CHUNK_SIZE - 1, y, z);

                if !left.is_opaque() {
                    mask |= 0b1 << 0;
                }

                if !right.is_opaque() {
                    mask |= 0b1 << 1;
                }
            }
        }

        // Bottom and top
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let bottom = reader.get(x, 0, z);
                let top = reader.get(x, CHUNK_SIZE - 1, z);

                if !bottom.is_opaque() {
                    mask |= 0b1 << 2;
                }

                if !top.is_opaque() {
                    mask |= 0b1 << 3;
                }
            }
        }

        // Back and front
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                let back = reader.get(x, y, 0);
                let front = reader.get(x, y, CHUNK_SIZE - 1);

                if !back.is_opaque() {
                    mask |= 0b1 << 4;
                }

                if !front.is_opaque() {
                    mask |= 0b1 << 5;
                }
            }
        }

        drop(reader); // Explicitly drop reader to release borrow
        self.visibility_mask = mask ^ 0b111111;
    }

    pub fn is_face_opaque(&self, face: Face) -> bool {
        self.visibility_mask & (0b1 << face.as_face_number()) != 0
    }

    pub fn build(&self) -> Mesh {
        let reader = self.reader();

        // Add padding to the chunk data
        let mut chunk_data = vec![Voxel::Empty; ChunkNDShapePadded::SIZE as usize];
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let index = ChunkNDShapePadded::linearize([x as u32 + 1, y as u32 + 1, z as u32 + 1]);
                    chunk_data[index as usize] = reader.get(x, y, z).clone();
                }
            }
        }  

        // Generate the mesh
        let mut buffer = GreedyQuadsBuffer::new(chunk_data.len());
        let faces = RIGHT_HANDED_Y_UP_CONFIG.faces;
        greedy_quads(
            &chunk_data,
            &ChunkNDShapePadded {},
            [0; 3],
            [CHUNK_SIZE as u32 + 1; 3],
            &faces,
            &mut buffer,
        );

        // Convert the mesh to a bevy mesh
        let mut mesh = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList);

        let num_indices = buffer.quads.num_quads() * 6;
        let num_vertices = buffer.quads.num_quads() * 4;

        let mut indices = Vec::with_capacity(num_indices);
        let mut positions = Vec::with_capacity(num_vertices);
        let mut normals = Vec::with_capacity(num_vertices);

        for (group, face) in buffer.quads.groups.into_iter().zip(faces.into_iter()) {
            for quad in group.into_iter() {
                indices.extend_from_slice(&face.quad_mesh_indices(positions.len() as u32));
                let _positions = &face.quad_mesh_positions(&quad, 1.0);
                // Translate positions to remove padding
                let _positions = _positions.iter().map(|pos| [pos[0] - 1.0, pos[1] - 1.0, pos[2] - 1.0]).collect::<Vec<[f32; 3]>>();
                positions.extend_from_slice(&_positions);
                normals.extend_from_slice(&face.quad_mesh_normals()); 
            }
        }

        mesh.set_indices(Some(bevy::render::mesh::Indices::U32(indices)));
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(positions));
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, VertexAttributeValues::Float32x3(normals));

        mesh
    }

    pub fn generate_with(&mut self, generator: impl Fn(&ChunkPosition, Vec3) -> Voxel) {
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    self.set(Vec3::new(x as f32, y as f32, z as f32), generator(&self.position, Vec3::new(x as f32, y as f32, z as f32)));
                }
            }
        }
    }
}

pub struct ChunkDataReader<'a> {
    data: RwLockReadGuard<'a, ChunkVoxels>
}

pub struct ChunkDataWriter<'a> {
    data: RwLockWriteGuard<'a, ChunkVoxels>
}

impl<'a> ChunkDataReader<'a> {
    pub fn get(&self, x: usize, y: usize, z: usize) -> &Voxel {
        let index = Chunk::linearize_position(x, y, z);
        self.data.get(index).unwrap()
    }
}

impl<'a> ChunkDataWriter<'a> {
    pub fn get(&mut self, x: usize, y: usize, z: usize) -> &mut Voxel {
        let index = Chunk::linearize_position(x, y, z);
        self.data.get_mut(index).unwrap()
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, voxel: Voxel) {
        let index = Chunk::linearize_position(x, y, z);
        self.data[index] = voxel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_opaque() {
        let mut chunk = Chunk::new(ChunkPosition::new(0, 0, 0));
        // Fill the top layer with opaque voxels
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set(Vec3::new(x as f32, CHUNK_SIZE as f32 - 1.0, z as f32), Voxel::NonEmpty { is_opaque: true });
            }
        }

        chunk.recalculate_visibility_mask();

        assert!(chunk.is_face_opaque(Face::Top));
        assert!(!chunk.is_face_opaque(Face::Bottom));
        assert!(!chunk.is_face_opaque(Face::Left));
    }
}