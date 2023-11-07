# Simple rust voxel rendering in bevy

This is a simple voxel renderer written in rust using the bevy game engine. It is based on optimizations from [Tommo's blog](https://tomcc.github.io/2014/08/31/visibility-2.html).

There is much more to be done, but it is a good start.

# Optimizations

Current optimizations include:
| Optimization | Description |
| --- | --- |
| Connection Masks | Keeping track of which faces are fully opaque and which are fully transparent |
| Opaqueness culling | Rendering from player position outwards, stopping at fully opaque faces |
| Back culling | If V = (player position) - (chunk position), then skip neighbor chunks where V dot N < 0, where N is the normal of the face |
| Frustum culling | Only meshing chunks that are within the camera's frustum |

There will be more optimizations to come in the future.
