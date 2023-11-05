use bevy::prelude::Vec3;

pub mod octree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Top,
    Bottom,
    Left,
    Right,
    Front,
    Back,
}

impl Face {
    pub fn opposite(&self) -> Self {
        match self {
            Face::Top => Face::Bottom,
            Face::Bottom => Face::Top,
            Face::Left => Face::Right,
            Face::Right => Face::Left,
            Face::Front => Face::Back,
            Face::Back => Face::Front,
        }
    }

    pub fn as_num(&self) -> usize {
        match self {
            Face::Top => 0,
            Face::Bottom => 1,
            Face::Left => 2,
            Face::Right => 3,
            Face::Front => 4,
            Face::Back => 5,
        }
    }

    pub fn normal(&self) -> Vec3 {
        match self {
            Face::Top => Vec3::new(0.0, 1.0, 0.0),
            Face::Bottom => Vec3::new(0.0, -1.0, 0.0),
            Face::Left => Vec3::new(-1.0, 0.0, 0.0),
            Face::Right => Vec3::new(1.0, 0.0, 0.0),
            Face::Front => Vec3::new(0.0, 0.0, 1.0),
            Face::Back => Vec3::new(0.0, 0.0, -1.0),
        }
    }
}