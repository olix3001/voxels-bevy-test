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
}