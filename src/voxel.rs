use block_mesh::VoxelVisibility;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voxel {
    pub is_opaque: bool,
}

impl Voxel {
    pub fn opaque() -> Self {
        Voxel {
            is_opaque: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalVoxel {
    Voxel(Voxel),
    Empty,
}

impl From<Option<Voxel>> for OptionalVoxel {
    fn from(voxel: Option<Voxel>) -> Self {
        match voxel {
            Some(voxel) => OptionalVoxel::Voxel(voxel),
            None => OptionalVoxel::Empty,
        }
    }
}

impl block_mesh::Voxel for OptionalVoxel {
    fn get_visibility(&self) -> VoxelVisibility {
        match self {
            OptionalVoxel::Voxel(voxel) => voxel.get_visibility(),
            OptionalVoxel::Empty => VoxelVisibility::Empty,
        }
    }
}

impl block_mesh::MergeVoxel for OptionalVoxel {
    type MergeValue = Self;

    fn merge_value(&self) -> Self::MergeValue {
        *self
    }
}

impl block_mesh::Voxel for Voxel {
    fn get_visibility(&self) -> VoxelVisibility {
        if self.is_opaque {
            VoxelVisibility::Opaque
        } else {
            VoxelVisibility::Translucent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_mesh::Voxel;

    #[test]
    fn test_voxel_visibility() {
        let opaque = super::Voxel::opaque();
        assert_eq!(opaque.get_visibility(), VoxelVisibility::Opaque);
    }

    #[test]
    fn test_voxel_equal() {
        let opaque = super::Voxel::opaque();
        assert_eq!(opaque, super::Voxel::opaque());
    }

    #[test]
    fn test_optional_voxel_from_option() {
        let opaque_voxel = super::Voxel::opaque();
        let voxel = Some(opaque_voxel);
        let optional_voxel: OptionalVoxel = voxel.into();
        assert_eq!(optional_voxel, OptionalVoxel::Voxel(super::Voxel::opaque()));
    }
}