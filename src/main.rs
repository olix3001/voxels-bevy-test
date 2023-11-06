use bevy::{prelude::*, pbr::wireframe::{WireframePlugin, WireframeConfig}, diagnostic::{LogDiagnosticsPlugin, FrameTimeDiagnosticsPlugin}};
use flycam::prelude::debug::DebugPlugin;

mod flycam;
pub mod engine;
mod debug;

fn setup(
    mut commands: Commands, 
    mut meshes: ResMut<Assets<Mesh>>, 
    mut materials: ResMut<Assets<StandardMaterial>>, 
    mut ambient_light: ResMut<AmbientLight>) {

    // Insert cube to mark origin
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
        .add_plugins(DebugPlugin)
        .add_plugins(flycam::PlayerPlugin)
        .add_plugins(engine::ChunkPlugin)
        .add_systems(Startup, setup)
        .run();
}
