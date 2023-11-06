#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voxel {
    Empty,
    NonEmpty {
        is_opaque: bool,
    }
}

impl Default for Voxel {
    fn default() -> Self {
        Self::Empty
    }
}

impl Voxel {
    pub fn is_opaque(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::NonEmpty { is_opaque } => *is_opaque,
        }
    }
}

impl block_mesh::Voxel for Voxel {
    fn get_visibility(&self) -> block_mesh::VoxelVisibility {
        match self {
            Self::Empty => block_mesh::VoxelVisibility::Empty,
            Self::NonEmpty { is_opaque } => {
                if *is_opaque {
                    block_mesh::VoxelVisibility::Opaque
                } else {
                    block_mesh::VoxelVisibility::Translucent
                }
            }
        }
    }
}

impl block_mesh::MergeVoxel for Voxel {
    type MergeValue = Self;

    fn merge_value(&self) -> Self::MergeValue {
        *self
    }
}