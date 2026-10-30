// M0 shading: a sun plus a sky/ground hemisphere ambient, with a soft
// highlight. Shadows, sky and fog arrive in M1.

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // Direction towards the sun, normalised.
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
};

struct Object {
    // Rotation, translation and uniform scale only, so it also transforms normals.
    model: mat4x4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> object: Object;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let world = object.model * vec4<f32>(v.position, 1.0);
    var out: VertexOut;
    out.clip = globals.view_proj * world;
    out.world_pos = world.xyz;
    out.normal = (object.model * vec4<f32>(v.normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = globals.sun_dir.xyz;
    let v = normalize(globals.eye.xyz - in.world_pos);
    let h = normalize(l + v);

    let diffuse = max(dot(n, l), 0.0);
    let ambient = mix(globals.ground_color.rgb, globals.sky_color.rgb, n.y * 0.5 + 0.5);
    let highlight = pow(max(dot(n, h), 0.0), 48.0) * 0.2 * diffuse;

    let base = object.color.rgb;
    let lit = base * (ambient + globals.sun_color.rgb * diffuse) + globals.sun_color.rgb * highlight;
    return vec4<f32>(lit, 1.0);
}
