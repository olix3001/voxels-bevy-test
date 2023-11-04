use bevy::{prelude::*, render::{mesh::{VertexAttributeValues, Indices}, render_resource::PrimitiveTopology}};
use crate::{util::octree::VoxelOctree, voxel::Voxel};

pub const CHUNK_SIZE: usize = 16;

#[derive(Component)]
pub struct Chunk {
    position: Vec3,
    octree: VoxelOctree<Voxel>,
}

impl Chunk {
    pub fn new(position: Vec3) -> Self {
        Chunk {
            position,
            octree: VoxelOctree::new(CHUNK_SIZE).unwrap(),
        }
    }

    pub fn insert(&mut self, position: Vec3, voxel: Voxel) {
        self.octree.insert(position, voxel);
    }

    pub fn get(&self, position: Vec3) -> Option<&Voxel> {
        self.octree.get(position)
    }

    pub fn generate_mesh(&self) -> Mesh {
        let mut mesh: Option<Mesh> = None;

        // This is most basic implementation of generating a mesh from a voxel octree.
        // TODO: Optimize this and add greedy meshing, LoD, etc.
        for x in 0..self.octree.size {
            for y in 0..self.octree.size {
                for z in 0..self.octree.size {
                    let voxel = self.octree.get(Vec3::new(x as f32, y as f32, z as f32));

                    if let Some(_) = voxel {
                        let cube_mesh = Mesh::from(shape::Cube { size: 1.0 });

                        println!("Creating cube at {}, {}, {}", x, y, z);
                        
                        if mesh.is_none() {
                            mesh = Some(cube_mesh);
                        } else {
                            mesh = Some(combine_meshes(
                                &[mesh.unwrap(), cube_mesh],
                                &[Transform::from_xyz(0.0, 0.0, 0.0) ,Transform::from_xyz(x as f32, y as f32, z as f32)],
                                true,
                                false,
                                false,
                                false,
                            ));
                        }
                    }
                }
            }
        }

        mesh.unwrap()
    }
}

fn combine_meshes(
    meshes: &[Mesh],
    transforms: &[Transform],
    use_normals: bool,
    use_tangents: bool,
    use_uvs: bool,
    use_colors: bool,
) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut tangets: Vec<[f32; 4]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let mut indices_offset = 0;

    if meshes.len() != transforms.len() {
        panic!(
            "meshes.len({}) != transforms.len({})",
            meshes.len(),
            transforms.len()
        );
    }

    for (mesh, trans) in meshes.iter().zip(transforms) {
        if let Indices::U32(mesh_indices) = &mesh.indices().unwrap() {
            let mat = trans.compute_matrix();

            let positions_len;

            if let Some(VertexAttributeValues::Float32x3(vert_positions)) =
                &mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                positions_len = vert_positions.len();
                for p in vert_positions {
                    positions.push(mat.transform_point3(Vec3::from(*p)).into());
                }
            } else {
                panic!("no positions")
            }

            if use_uvs {
                if let Some(VertexAttributeValues::Float32x2(vert_uv)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_UV_0)
                {
                    for uv in vert_uv {
                        uvs.push(*uv);
                    }
                } else {
                    panic!("no uvs")
                }
            }

            if use_normals {
                // Comment below taken from mesh_normal_local_to_world() in mesh_functions.wgsl regarding
                // transform normals from local to world coordinates:

                // NOTE: The mikktspace method of normal mapping requires that the world normal is
                // re-normalized in the vertex shader to match the way mikktspace bakes vertex tangents
                // and normal maps so that the exact inverse process is applied when shading. Blender, Unity,
                // Unreal Engine, Godot, and more all use the mikktspace method. Do not change this code
                // unless you really know what you are doing.
                // http://www.mikktspace.com/

                let inverse_transpose_model = mat.inverse().transpose();
                let inverse_transpose_model = Mat3 {
                    x_axis: inverse_transpose_model.x_axis.xyz(),
                    y_axis: inverse_transpose_model.y_axis.xyz(),
                    z_axis: inverse_transpose_model.z_axis.xyz(),
                };

                if let Some(VertexAttributeValues::Float32x3(vert_normals)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                {
                    for n in vert_normals {
                        normals.push(
                            inverse_transpose_model
                                .mul_vec3(Vec3::from(*n))
                                .normalize_or_zero()
                                .into(),
                        );
                    }
                } else {
                    panic!("no normals")
                }
            }

            if use_tangents {
                if let Some(VertexAttributeValues::Float32x4(vert_tangets)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
                {
                    for t in vert_tangets {
                        tangets.push(*t);
                    }
                } else {
                    panic!("no tangets")
                }
            }

            if use_colors {
                if let Some(VertexAttributeValues::Float32x4(vert_colors)) =
                    &mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                {
                    for c in vert_colors {
                        colors.push(*c);
                    }
                } else {
                    panic!("no colors")
                }
            }

            for i in mesh_indices {
                indices.push(*i + indices_offset);
            }
            indices_offset += positions_len as u32;
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    if use_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }

    if use_tangents {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangets);
    }

    if use_uvs {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }

    if use_colors {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }

    mesh.set_indices(Some(Indices::U32(indices)));

    mesh
}