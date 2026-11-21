//! wgpu 设备探测、计算管线与一致性/基准测试。
//!
//! 任何初始化或执行失败都会返回错误，由上层回退 CPU；
//! 这里不吞掉错误，也不假装 GPU 可用。

use std::time::Instant;

use serde::Serialize;
use wgpu::util::DeviceExt;

use super::ops::{identity_lut, RgbaF32Image};

const EXPOSURE_WGSL: &str = include_str!("shaders/exposure.wgsl");
const CURVE_WGSL: &str = include_str!("shaders/curve.wgsl");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuCapabilities {
    pub device_name: String,
    pub backend: String,
    pub driver: String,
    pub device_type: String,
    pub max_texture_dimension_2d: u32,
    pub max_storage_buffer_binding_size: u64,
    pub max_buffer_size: u64,
}

pub struct GpuEngine {
    device: wgpu::Device,
    queue: wgpu::Queue,
    exposure: wgpu::ComputePipeline,
    curve: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    capabilities: GpuCapabilities,
}

impl GpuEngine {
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL | wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("未找到可用 GPU 适配器：{error}"))?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lumi-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
                max_buffer_size: limits.max_buffer_size,
                ..wgpu::Limits::default()
            },
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("创建设备失败：{error}"))?;

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lumi-compute-layout"),
            entries: &[
                storage_entry(0, true),
                storage_entry(1, false),
                storage_entry(2, true),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lumi-compute-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let exposure = create_pipeline(&device, &pipeline_layout, EXPOSURE_WGSL, "exposure")?;
        let curve = create_pipeline(&device, &pipeline_layout, CURVE_WGSL, "curve")?;

        let capabilities = GpuCapabilities {
            device_name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            driver: info.driver.clone(),
            device_type: format!("{:?}", info.device_type),
            max_texture_dimension_2d: limits.max_texture_dimension_2d,
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
        };
        Ok(Self {
            device,
            queue,
            exposure,
            curve,
            layout,
            capabilities,
        })
    }

    pub fn capabilities(&self) -> &GpuCapabilities {
        &self.capabilities
    }

    /// GPU 曝光/色彩矩阵。
    pub fn apply_exposure(
        &self,
        image: &RgbaF32Image,
        exposure_ev: f32,
        matrix: Option<[[f32; 3]; 3]>,
    ) -> Result<RgbaF32Image, String> {
        let mut params = vec![0.0f32; 13];
        params[0] = exposure_ev;
        if let Some(matrix) = matrix {
            params[10] = 1.0;
            for row in 0..3 {
                for col in 0..3 {
                    params[1 + row * 3 + col] = matrix[row][col];
                }
            }
        }
        params[11] = image.width as f32;
        params[12] = image.height as f32;
        self.dispatch(&self.exposure, image, &params)
    }

    /// GPU 曲线/饱和度。
    pub fn apply_curve(
        &self,
        image: &RgbaF32Image,
        lut: &[f32; 256],
        saturation: f32,
    ) -> Result<RgbaF32Image, String> {
        let mut params = vec![0.0f32; 771];
        params[..256].copy_from_slice(lut);
        params[768] = saturation;
        params[769] = image.width as f32;
        params[770] = image.height as f32;
        self.dispatch(&self.curve, image, &params)
    }

    fn dispatch(
        &self,
        pipeline: &wgpu::ComputePipeline,
        image: &RgbaF32Image,
        params: &[f32],
    ) -> Result<RgbaF32Image, String> {
        if image.width == 0 || image.height == 0 {
            return Ok(image.clone());
        }
        let pixel_bytes = std::mem::size_of_val(&image.data[0]) as u64;
        let data_size = image.data.len() as u64 * pixel_bytes;
        let max_binding = self.capabilities.max_storage_buffer_binding_size;
        if data_size > max_binding {
            return Err(format!(
                "图像数据 {data_size} 字节超过 GPU 单次绑定上限 {max_binding}"
            ));
        }

        let bytes = bytemuck::cast_slice(&image.data);
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lumi-input"),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lumi-output"),
            size: data_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lumi-staging"),
            size: data_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let param_bytes = bytemuck::cast_slice(params);
        let param_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lumi-params"),
                contents: param_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lumi-bind-group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: param_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("lumi-encoder"),
                });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("lumi-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(image.width.div_ceil(16), image.height.div_ceil(16), 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, data_size);
        self.queue.submit(Some(encoder.finish()));

        let slice = staging.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("等待 GPU 失败：{error}"))?;
        match receiver.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(format!("读回 GPU 数据失败：{error}")),
            Err(error) => return Err(format!("GPU 回调丢失：{error}")),
        }
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| format!("映射 GPU 数据失败：{error}"))?;
        let data: Vec<[f32; 4]> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging.unmap();

        Ok(RgbaF32Image {
            width: image.width,
            height: image.height,
            data,
        })
    }

    /// 端到端基准：CPU 两项运算 vs GPU 上传+运算+读回。
    pub fn benchmark(&self, width: u32, height: u32) -> Result<BenchRow, String> {
        let mut image = RgbaF32Image::new(width, height);
        for (index, pixel) in image.data.iter_mut().enumerate() {
            let x = (index as u32 % width) as f32 / width as f32;
            let y = (index as u32 / width) as f32 / height as f32;
            *pixel = [x, y, 0.5 * (x + y), 1.0];
        }
        let lut = identity_lut();

        let cpu_start = Instant::now();
        let cpu_exposed = super::ops::cpu_exposure(&image, 0.5, None);
        let _ = super::ops::cpu_curve(&cpu_exposed, &lut, 0.2);
        let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1000.0;

        let upload_start = Instant::now();
        let bytes: Vec<u8> = bytemuck::cast_slice(&image.data).to_vec();
        let upload_ms = upload_start.elapsed().as_secs_f64() * 1000.0;

        let compute_start = Instant::now();
        let exposed = self.apply_exposure(&image, 0.5, None)?;
        let _ = self.apply_curve(&exposed, &lut, 0.2)?;
        let gpu_total = compute_start.elapsed().as_secs_f64() * 1000.0;

        Ok(BenchRow {
            width,
            height,
            megapixels: (width as f64 * height as f64 / 1_000_000.0 * 10.0).round() / 10.0,
            cpu_ms: round3(cpu_ms),
            gpu_upload_ms: round3(upload_ms),
            gpu_total_ms: round3(gpu_total),
            gpu_compute_ms: round3((gpu_total - upload_ms).max(0.0)),
            speedup: if gpu_total > 0.0 {
                round3(cpu_ms / gpu_total)
            } else {
                0.0
            },
            upload_bytes: bytes.len() as u64,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchRow {
    pub width: u32,
    pub height: u32,
    pub megapixels: f64,
    pub cpu_ms: f64,
    pub gpu_upload_ms: f64,
    pub gpu_compute_ms: f64,
    pub gpu_total_ms: f64,
    pub speedup: f64,
    pub upload_bytes: u64,
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    label: &str,
) -> Result<wgpu::ComputePipeline, String> {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    Ok(device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    }))
}
