use bevy::prelude::Vec3;

/// Error that can occur when creating a voxel octree.
#[derive(Debug)]
pub enum VoxelOctreeCreationError {
    /// The size of the octree is not a power of 2.
    SizeNotPowerOfTwo,
}

#[derive(Debug)]
/// Specialized octree for storing voxels.
pub struct VoxelOctree<T> {
    /// Size of the octree.
    size: usize,
    /// The root node of the octree.
    root: Octree<T>,
}

impl<T> VoxelOctree<T> {
    /// Create a new octree.
    pub fn new(size: usize) -> Result<Self, VoxelOctreeCreationError> {
        // Ensure that the size is a power of 2.
        if !size.is_power_of_two() {
            return Err(VoxelOctreeCreationError::SizeNotPowerOfTwo);
        }

        Ok(VoxelOctree {
            size,
            root: Octree::new(),
        })
    }

    /// Insert a value into the octree at the given position dividing the octree if necessary.
    pub fn insert(&mut self, position: Vec3, value: T) {
        let mut current_size = self.size;
        let mut current_node = &mut self.root;
        let mut current_position = position;

        while current_size > 1 {
            // Get the octant index for the given position.
            let octant_index = Self::get_octant_index(current_position, current_size);

            // If current node is empty, create a new node.
            if let Octree::Empty = current_node {
                *current_node = Octree::Node(Box::new([
                    Octree::Empty, Octree::Empty, Octree::Empty, Octree::Empty,
                    Octree::Empty, Octree::Empty, Octree::Empty, Octree::Empty,
                ]));
            }

            // Set the current node to the child node.
            if let Octree::Node(children) = current_node {
                current_node = &mut children[octant_index];
            } else {
                unreachable!();
            }

            // Divide the octree
            current_size /= 2;
            // Set the current position to the position of the octant.
            current_position -= Vec3::new(
                if octant_index & 1 == 1 { current_size as f32 } else { 0.0 },
                if octant_index & 2 == 2 { current_size as f32 } else { 0.0 },
                if octant_index & 4 == 4 { current_size as f32 } else { 0.0 },
            );
        }

        // Finally set the value of the leaf node.
        *current_node = Octree::Leaf(value);
    }

    /// Get the value at the given position.
    pub fn get_value(&self, position: Vec3) -> Option<&T> {
        let mut current_size = self.size;
        let mut current_node = &self.root;
        let mut current_position = position;

        while current_size > 1 {
            // Get the octant index for the given position.
            let octant_index = Self::get_octant_index(current_position, current_size);

            // Set the current node to the child node.
            if let Octree::Node(children) = current_node {
                current_node = &children[octant_index];
            } else {
                return None;
            }

            // Divide the octree
            current_size /= 2;
            // Set the current position to the position of the octant.
            current_position -= Vec3::new(
                if octant_index & 1 == 1 { current_size as f32 } else { 0.0 },
                if octant_index & 2 == 2 { current_size as f32 } else { 0.0 },
                if octant_index & 4 == 4 { current_size as f32 } else { 0.0 },
            );
        }

        // Finally get the value of the leaf node.
        if let Octree::Leaf(value) = current_node {
            Some(value)
        } else {
            None
        }
    }

    /// Get the octant index for the given position (positioned from top-left-front to bottom-right-back)
    fn get_octant_index(position: Vec3, size: usize) -> usize {
        let mut index = 0;

        if position.x >= size as f32 / 2.0 {
            index |= 1;
        }

        if position.y >= size as f32 / 2.0 {
            index |= 2;
        }

        if position.z >= size as f32 / 2.0 {
            index |= 4;
        }

        index
    }
}

#[derive(Debug)]
/// Octree implementation for storing any type of data.
pub enum Octree<T> {
    /// Empty variant.
    Empty,
    /// A leaf node in the octree.
    Leaf(T),
    /// A node in the octree with 8 children.
    Node(Box<[Octree<T>; 8]>),
}

impl<T> Octree<T> {
    /// Create a new octree.
    pub fn new() -> Self {
        Octree::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_octree_creation() {
        let octree = VoxelOctree::<u32>::new(8).unwrap();
        assert_eq!(octree.size, 8);
    }

    #[test]
    fn test_octree_creation_error() {
        let octree = VoxelOctree::<u32>::new(7);
        assert!(octree.is_err());
    }

    #[test]
    fn test_octree_insert() {
        let mut octree = VoxelOctree::<u32>::new(8).unwrap();
        octree.insert(Vec3::new(0.0, 0.0, 0.0), 1);
        octree.insert(Vec3::new(1.0, 0.0, 0.0), 2);
        octree.insert(Vec3::new(0.0, 1.0, 0.0), 3);
        octree.insert(Vec3::new(1.0, 8.0, 0.0), 4);
        octree.insert(Vec3::new(4.0, 7.0, 3.0), 5);
        
        assert_eq!(octree.get_value(Vec3::new(0.0, 0.0, 0.0)), Some(&1));
        assert_eq!(octree.get_value(Vec3::new(1.0, 0.0, 0.0)), Some(&2));
        assert_eq!(octree.get_value(Vec3::new(0.0, 1.0, 0.0)), Some(&3));
        assert_eq!(octree.get_value(Vec3::new(1.0, 8.0, 0.0)), Some(&4));
        assert_eq!(octree.get_value(Vec3::new(4.0, 7.0, 3.0)), Some(&5));
    }
}