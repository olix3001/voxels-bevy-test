use bevy::prelude::*;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(debug_assertions)]
        app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
            .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin::default());
    }
}