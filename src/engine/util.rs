use bevy::prelude::Vec3;

use super::chunk::{ChunkPosition, CHUNK_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Left,
    Right,
    Bottom,
    Top,
    Back,
    Front,
}

impl Face {
    pub fn opposite(&self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Bottom => Self::Top,
            Self::Top => Self::Bottom,
            Self::Back => Self::Front,
            Self::Front => Self::Back,
        }
    }

    pub fn as_face_number(&self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Bottom => 2,
            Self::Top => 3,
            Self::Back => 4,
            Self::Front => 5,
        }
    }

    pub fn normal(&self) -> Vec3 {
        match self {
            Self::Left => Vec3::new(-1.0, 0.0, 0.0),
            Self::Right => Vec3::new(1.0, 0.0, 0.0),
            Self::Bottom => Vec3::new(0.0, -1.0, 0.0),
            Self::Top => Vec3::new(0.0, 1.0, 0.0),
            Self::Back => Vec3::new(0.0, 0.0, -1.0),
            Self::Front => Vec3::new(0.0, 0.0, 1.0),
        }
    }

    pub fn face_center_in_chunk(&self, chunk: &ChunkPosition) -> Vec3 {
        chunk.as_world_position() + match self {
            Self::Left => Vec3::new(0.0, 0.5, 0.5),
            Self::Right => Vec3::new(1.0, 0.5, 0.5),
            Self::Bottom => Vec3::new(0.5, 0.0, 0.5),
            Self::Top => Vec3::new(0.5, 1.0, 0.5),
            Self::Back => Vec3::new(0.5, 0.5, 0.0),
            Self::Front => Vec3::new(0.5, 0.5, 1.0),
        } * CHUNK_SIZE as f32
    }
}