use bevy::prelude::*;

pub mod util;
pub mod chunk;
pub mod voxel;
mod flycam;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut ambientLight: ResMut<AmbientLight>) {
    // commands.spawn(Camera3dBundle::default());

    let mut chunk = chunk::Chunk::new(Vec3::new(0.0, 0.0, 0.0));
    chunk.insert(Vec3::new(0.0, 0.0, 0.0), voxel::Voxel {});
    chunk.insert(Vec3::new(2.0, 0.0, 0.0), voxel::Voxel {});

    let mesh = chunk.generate_mesh();
    let mesh_handle = meshes.add(mesh.clone());
    let material_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.5, 0.5, 1.0),
        ..Default::default()
    });

    commands.spawn(PbrBundle {
        mesh: mesh_handle,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        material: material_handle,
        ..Default::default()
    });

    ambientLight.brightness = 0.5;
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(flycam::PlayerPlugin)
        .add_systems(Startup, setup)
        .run();
}
