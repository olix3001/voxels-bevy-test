use bevy::{prelude::*, pbr::wireframe::{WireframePlugin, WireframeConfig}};
use flycam::prelude::voxel::Voxel;

pub mod util;
pub mod chunk;
pub mod voxel;
mod flycam;

fn setup(
    mut commands: Commands, 
    mut meshes: ResMut<Assets<Mesh>>, 
    mut materials: ResMut<Assets<StandardMaterial>>, 
    mut ambient_light: ResMut<AmbientLight>) {
    // commands.spawn(Camera3dBundle::default());

    let mut chunk = chunk::Chunk::outlined();

    let mesh = chunk.generate_mesh(1);
    let mesh_handle = meshes.add(mesh.clone());
    let material_handle = materials.add(StandardMaterial {
        base_color: Color::rgb(0.2, 0.8, 0.35),
        perceptual_roughness: 0.1,
        ..Default::default()
    });

    commands.spawn(PbrBundle {
        mesh: mesh_handle,
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        material: material_handle,
        ..Default::default()
    });

    // Insert sphere to mark origin
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube {
            size: 0.1,
        })),
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
        ..Default::default()
    });

    ambient_light.brightness = 0.7;
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(WireframePlugin)
        .insert_resource(WireframeConfig {
            global: true,
            ..Default::default()
        })
        .add_plugins(flycam::PlayerPlugin)
        .add_systems(Startup, setup)
        .run();
}
