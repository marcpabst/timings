@vertex
fn vs_main(@builtin(vertex_index) ix: u32) -> @builtin(position) vec4<f32> {
    // Generate a full screen quad in normalized device coordinates
    var vertex = vec2(-1.0, 1.0);
    switch ix {
        case 1u: {
            vertex = vec2(-1.0, -1.0);
        }
        case 2u, 4u: {
            vertex = vec2(1.0, -1.0);
        }
        case 5u: {
            vertex = vec2(1.0, 1.0);
        }
        default: {}
    }
    return vec4(vertex, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}