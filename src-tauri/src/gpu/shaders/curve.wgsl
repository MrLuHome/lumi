// 曲线 LUT + 饱和度，与 Rust 端 `cpu_curve` 数学一致。
// params: [0..256]=LUT, [768]=saturation, [769]=width, [770]=height

struct Params {
    values: array<f32>,
};

@group(0) @binding(0) var<storage, read> input: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> params: Params;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = u32(params.values[769]);
    let height = u32(params.values[770]);
    if (id.x >= width || id.y >= height) {
        return;
    }
    let index = id.y * width + id.x;
    let src = input[index];
    let clamped = clamp(src.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let index_r = u32(round(clamped.r * 255.0));
    let index_g = u32(round(clamped.g * 255.0));
    let index_b = u32(round(clamped.b * 255.0));
    var c = vec3<f32>(
        params.values[index_r],
        params.values[index_g],
        params.values[index_b],
    );
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    c = mix(vec3<f32>(luma), c, 1.0 + params.values[768]);
    output[index] = vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);
}
