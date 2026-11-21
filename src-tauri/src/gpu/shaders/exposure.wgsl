// 曝光 + 色彩矩阵（线性空间），与 Rust 端 `cpu_exposure` 数学一致。
// params: [0]=exposure_ev, [1..10]=行主序 3x3 矩阵, [10]=是否启用矩阵, [11]=width, [12]=height

struct Params {
    values: array<f32>,
};

@group(0) @binding(0) var<storage, read> input: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> params: Params;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = u32(params.values[11]);
    let height = u32(params.values[12]);
    if (id.x >= width || id.y >= height) {
        return;
    }
    let index = id.y * width + id.x;
    let src = input[index];
    var c = srgb_to_linear(clamp(src.rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
    c = c * exp2(params.values[0]);
    if (params.values[10] > 0.5) {
        let m = mat3x3<f32>(
            vec3<f32>(params.values[1], params.values[2], params.values[3]),
            vec3<f32>(params.values[4], params.values[5], params.values[6]),
            vec3<f32>(params.values[7], params.values[8], params.values[9]),
        );
        c = m * c;
    }
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    output[index] = vec4<f32>(linear_to_srgb(c), src.a);
}
