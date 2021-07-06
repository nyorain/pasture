use wgpu::util::{DeviceExt, BufferInitDescriptor};
use wgpu::BufferDescriptor;
use crate::layout::{PointAttributeDataType};
use crate::layout;
use crate::containers::{PointBuffer};
use bytemuck::__core::convert::TryInto;

pub struct Device {
    // Private fields
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    upload_buffers: Vec<wgpu::Buffer>,
    download_buffers: Vec<wgpu::Buffer>,
    buffer_sizes: Vec<wgpu::BufferAddress>,
    buffer_bindings: Vec<u32>,

    cs_module: Option<wgpu::ShaderModule>,
    bind_group: Option<wgpu::BindGroup>,
    compute_pipeline: Option<wgpu::ComputePipeline>,
}

impl Device {
    /// Create a device with default options:
    /// - Low power GPU
    /// - Primary backend for wgpu to use [Vulkan, Metal, Dx12, Browser]
    pub async fn default() -> Device {
        Device::new(DeviceOptions::default()).await
    }

    /// Create a device respecting the desired [DeviceOptions]
    pub async fn new(device_options: DeviceOptions) -> Device {
        // == Create an instance from the desired backend =========================================

        let backend_bits = match device_options.device_backend {
            DeviceBackend::Primary => { wgpu::BackendBit::PRIMARY }
            DeviceBackend::Secondary => { wgpu::BackendBit::SECONDARY }
            DeviceBackend::Vulkan => { wgpu::BackendBit::VULKAN }
            DeviceBackend::Metal => { wgpu::BackendBit::METAL }
            DeviceBackend::Dx12 => { wgpu::BackendBit::DX12 }
            DeviceBackend::Dx11 => { wgpu::BackendBit::DX11 }
            DeviceBackend::OpenGL => { wgpu::BackendBit::GL }
            DeviceBackend::Browser => { wgpu::BackendBit::BROWSER_WEBGPU }
        };

        let instance = wgpu::Instance::new(backend_bits);

        // == Create an adapter with the desired power preference =================================

        let power_pref = if matches!(device_options.device_power, DevicePower::Low) {
            wgpu::PowerPreference::LowPower
        }
        else {
            wgpu::PowerPreference::HighPerformance
        };

        // The adapter gives us a handle to the actual device.
        // We can query some GPU information, such as the device name, its type (discrete vs integrated)
        // or the backend that is being used.
        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: power_pref,
                compatible_surface: None
            }
        ).await.unwrap();

        // == Create a device and a queue from the given adapter ==================================

        let features = if device_options.use_adapter_features {
            adapter.features()
        }
        else {
            wgpu::Features::default()
        };

        let limits = if device_options.use_adapter_limits {
            adapter.limits()
        }
        else {
            // Some important ones that may be worth altering:
            //  - max_storage_buffers_per_shader_stage (defaults to just 4)
            //  - max_uniform_buffers_per_shader_stage (defaults to 12, which seems fine)
            //  - ...
            wgpu::Limits::default()
        };

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features,
                limits,
            },
            None,
        ).await.unwrap();

        // == Initially empty buffers =============================================================

        let upload_buffers: Vec<wgpu::Buffer> = Vec::new();
        let download_buffers: Vec<wgpu::Buffer> = Vec::new();
        let buffer_sizes: Vec<wgpu::BufferAddress> = Vec::new();
        let buffer_bindings: Vec<u32> = Vec::new();

        let cs_module = Option::None;
        let bind_group = Option::None;
        let compute_pipeline = Option::None;

        Device {
            adapter,
            device,
            queue,
            upload_buffers,
            download_buffers,
            buffer_sizes,
            buffer_bindings,
            cs_module,
            bind_group,
            compute_pipeline,
        }
    }

    /// Prints name, type, backend, PCI and vendor PCI id of the device.
    pub fn print_device_info(&self) {
        let info = self.adapter.get_info();

        println!("== Device Information ========");
        println!("Name: {}", info.name);
        println!("Type: {:?}", info.device_type);
        println!("Backend: {:?}", info.backend);
        println!("PCI id: {}", info.device);
        println!("Vendor PCI id: {}\n", info.vendor);
    }

    // TODO: are these feature/limit prints useful?
    pub fn print_default_features(&self) {
        println!("{:?}", wgpu::Features::default());
    }

    pub fn print_adapter_features(&self) {
        println!("{:?}", self.adapter.features());
    }

    pub fn print_active_features(&self) {
        println!("{:?}", self.device.features());
    }

    pub fn print_default_limits(&self) {
        println!("{:?}", wgpu::Limits::default());
    }

    pub fn print_adapter_limits(&self) {
        println!("{:?}", self.adapter.limits());
    }

    pub fn print_active_limits(&self) {
        println!("{:?}", self.device.limits());
    }

    /// Associates the given `PointBuffer` with GPU buffers w.r.t. the layouts defined in `Vec<BufferInfo>`.
    pub fn upload(&mut self, buffer: &mut dyn PointBuffer, buffer_infos: Vec<BufferInfo>) {
        let len = buffer.len();

        for info in buffer_infos {
            let num_bytes = self.bytes_per_element(info.attribute.datatype()) as usize;
            let mut bytes_to_write: Vec<u8> = vec![0; len * num_bytes];

            buffer.get_raw_attribute_range(0..len, info.attribute, &mut *bytes_to_write);

            // Change Vec<u8> to &[u8]
            let bytes_to_write: &[u8] = &*bytes_to_write;
            let bytes_to_write = &self.align_slice(bytes_to_write, info.attribute.datatype())[..];

            let size_in_bytes = bytes_to_write.len() as wgpu::BufferAddress;
            self.buffer_sizes.push(size_in_bytes);

            self.upload_buffers.push(self.device.create_buffer_init(
                &BufferInitDescriptor {
                    label: None,
                    contents: bytes_to_write,
                    usage: wgpu::BufferUsage::STORAGE | wgpu::BufferUsage::COPY_SRC
                }
            ));

            self.download_buffers.push(self.device.create_buffer(
                &BufferDescriptor {
                    label: None,
                    size: size_in_bytes,
                    usage: wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_DST,
                    mapped_at_creation: false
                }
            ));

            self.buffer_bindings.push(info.binding);
        }
    }

    // Given a PointAttributeDataType, returns the number of bytes an element with such type would need
    fn bytes_per_element(&self, datatype: PointAttributeDataType) -> u32 {
        let num_bytes = match datatype {
            PointAttributeDataType::U8 => { 1 }
            PointAttributeDataType::I8 => { 1 }
            PointAttributeDataType::U16 => { 2 }
            PointAttributeDataType::I16 => { 2 }
            PointAttributeDataType::U32 => { 4 }
            PointAttributeDataType::I32 => { 4 }
            PointAttributeDataType::U64 => { 8 }
            PointAttributeDataType::I64 => { 8 }
            PointAttributeDataType::F32 => { 4 }
            PointAttributeDataType::F64 => { 8 }
            PointAttributeDataType::Bool => { 1 }
            PointAttributeDataType::Vec3u8 => { 3 }
            PointAttributeDataType::Vec3u16 => { 6 }
            PointAttributeDataType::Vec3f32 => { 12 }
            PointAttributeDataType::Vec3f64 => { 24 }
        };

        num_bytes
    }

    // Given a slice of bytes and the corresponding data type of those bytes,
    // will ensure the bytes match the std430 layout of GLSL.
    //
    // In particular:
    //  - Unsigned integer types with less than 32 bits will be zero extended to 32 bits
    //  - Signed integer types with less than 32 bits will be sign extended to 32 bits
    //  - Booleans will be zero extended to 32 bits
    //  - 32 bit signed or unsigned integer types will be taken as is
    //  - 32 bit and 64 bit floating point types will be taken as is
    //  - Vec3 will be treated as Vec4 with w-coordinate set to 1
    //  - Above extension rules apply to the elements of vectors
    //
    // Will panic if data type is a 64-bit integer.
    //
    // TODO: Consider whether to support such sign/zero extension or just forbid types that need them.
    fn align_slice(&self, slice: &[u8], datatype: PointAttributeDataType) -> Vec<u8> {
        let len = slice.len();

        match datatype {
            PointAttributeDataType::U8 => {
                // Convert to u32
                let mut v: Vec<u8> = Vec::new();
                for i in 0..len {
                    let current = slice[i] as u32;
                    v.extend_from_slice(&current.to_ne_bytes());
                }
                return v;
            }
            PointAttributeDataType::I8 => {
                // Convert to i32
                let mut v: Vec<u8> = Vec::new();
                for i in 0..len {
                    let current = i8::from_ne_bytes(slice[i..i+1].try_into().unwrap());
                    v.extend_from_slice(&(current as i32).to_ne_bytes());
                }
                return v;
            }
            PointAttributeDataType::U16 => {
                // Convert to u32
                let stride = self.bytes_per_element(datatype) as usize;
                let num_elements = len / stride;

                let mut v: Vec<u8> = Vec::new();
                for i in 0..num_elements {
                    let begin = i * stride;
                    let end = (i * stride) + stride;

                    let current = u16::from_ne_bytes(slice[begin..end].try_into().unwrap());
                    v.extend_from_slice(&(current as u32).to_ne_bytes());
                }
                return v;
            }
            PointAttributeDataType::I16 => {
                // Convert to i32
                let stride = self.bytes_per_element(datatype) as usize;
                let num_elements = len / stride;

                let mut v: Vec<u8> = Vec::new();
                for i in 0..num_elements {
                    let begin = i * stride;
                    let end = (i * stride) + stride;

                    let current = i16::from_ne_bytes(slice[begin..end].try_into().unwrap());
                    v.extend_from_slice(&(current as i32).to_ne_bytes());
                }
                return v;
            }
            PointAttributeDataType::U32 => {
                // Does not need any altering -> can directly be used as uint in shader
            }
            PointAttributeDataType::I32 => {
                // Does not need any altering -> can directly be used as int in shader
            }
            PointAttributeDataType::U64 => {
                // Trouble: no 64-bit integer types on GPU
                panic!("Uploading 64-bit integer types to the GPU is not supported.")
            }
            PointAttributeDataType::I64 => {
                // Trouble: no 64-bit integer types on GPU
                panic!("Uploading 64-bit integer types to the GPU is not supported.")
            }
            PointAttributeDataType::F32 => {
                // Does not need any altering -> can directly be used as float in shader
            }
            PointAttributeDataType::F64 => {
                // Does not need any altering -> can directly be used as double in shader
            }
            PointAttributeDataType::Bool => {
                // Convert to u32
                let mut v: Vec<u8> = Vec::new();
                for i in 0..len {
                    let current = slice[i] as u32;
                    v.extend_from_slice(&current.to_ne_bytes());
                }
                return v;
            }
            PointAttributeDataType::Vec3u8 => {
                // Convert to Vec4u32
                let one_as_bytes = 1_u32.to_ne_bytes();

                // Each entry is 8 bits, ie. 1 byte -> each Vec3 has 3 bytes
                let stride = self.bytes_per_element(datatype) as usize;
                let num_elements = len / stride;

                let mut v = Vec::new();
                for i in 0..num_elements {
                    // Iteration over each Vec3
                    for j in 0..3 {
                        // Extend each entry to 32 bits
                        let begin = (i * stride) + j;
                        let end = (i * stride) + j + 1;

                        let current = u8::from_ne_bytes(slice[begin..end].try_into().unwrap());
                        v.extend_from_slice(&(current as u32).to_ne_bytes());
                    }

                    // Append fourth coordinate
                    v.extend_from_slice(&one_as_bytes);
                }
                return v;
            }
            PointAttributeDataType::Vec3u16 => {
                // Convert to Vec4u32
                let one_as_bytes = 1_u32.to_ne_bytes();

                // Each entry is 16 bits, ie. 2 bytes -> each Vec3 has 3*2 = 6 bytes
                let stride = self.bytes_per_element(datatype) as usize;   // = 6
                let num_elements = len / stride;

                let mut v = Vec::new();
                for i in 0..num_elements {
                    // Iteration over each Vec3
                    for j in 0..3 {
                        // Extend each entry to 32 bits
                        let begin = (i * stride) + j * 2;
                        let end = (i * stride) + (j * 2) + 2;

                        let current = u16::from_ne_bytes(slice[begin..end].try_into().unwrap());
                        v.extend_from_slice(&(current as u32).to_ne_bytes());
                    }

                    // Append fourth coordinate
                    v.extend_from_slice(&one_as_bytes);
                }
                return v;
            }
            PointAttributeDataType::Vec3f32 => {
                // Make Vec4f32 by appending 1.0
                let one_as_bytes = 1.0_f32.to_ne_bytes();

                // Each entry is 64 bits and hence consists of 8 bytes -> a Vec3 has 24 bytes
                let stride = self.bytes_per_element(datatype) as usize;   // = 24
                let num_elements = len / stride;

                let mut v: Vec<u8> = Vec::new();
                for i in 0..num_elements {
                    let begin = i * stride;
                    let end = (i * stride) + stride;

                    // Push current Vec3
                    v.extend_from_slice(&slice[begin..end]);

                    // Push 1 as fourth coordinate
                    v.extend_from_slice(&one_as_bytes);
                }

                return v;
            }
            PointAttributeDataType::Vec3f64 => {
                // Make Vec4f64 by appending 1.0
                let one_as_bytes = 1.0_f64.to_ne_bytes();

                // Each entry is 64 bits and hence consists of 8 bytes -> a Vec3 has 24 bytes
                let stride = self.bytes_per_element(datatype) as usize;   // = 24
                let num_elements = len / stride;

                let mut v: Vec<u8> = Vec::new();
                for i in 0..num_elements {
                    let begin = i * stride;
                    let end = (i * stride) + stride;

                    // Push current Vec3
                    v.extend_from_slice(&slice[begin..end]);

                    // Push 1 as fourth coordinate
                    v.extend_from_slice(&one_as_bytes);
                }

                return v;
            }
        }

        Vec::from(slice)
    }

    /// Downloads contents of GPU buffers
    // TODO: Currently returns vector of bytes per buffer... instead return altered point buffer.
    pub async fn download(&self) -> Vec<Vec<u8>> {
        let mut output_bytes: Vec<Vec<u8>> = Vec::new();

        for i in 0..self.download_buffers.len() {
            let download = self.download_buffers.get(i).unwrap();

            let result_buffer_slice = download.slice(..);
            let result_buffer_future = result_buffer_slice.map_async(wgpu::MapMode::Read);
            self.device.poll(wgpu::Maintain::Wait); // TODO: "Should be called in event loop or other thread ..."

            // TODO: how to know the data type of the current buffer?
            if let Ok(()) = result_buffer_future.await {
                let result_as_bytes = result_buffer_slice.get_mapped_range();
                &output_bytes.push(result_as_bytes.to_vec());

                // Drop all mapped views before unmapping buffer
                drop(result_as_bytes);
                download.unmap();
            }
        };

        output_bytes
    }

    /// Compiles the given compute shader source code and constructs a compute pipeline for it.
    pub fn set_compute_shader(&mut self, compute_shader_src: &str) {
        self.cs_module = self.compile_and_create_compute_module(compute_shader_src);

        let (bind_group, pipeline)
            = self.create_compute_pipeline(self.cs_module.as_ref().unwrap());

        self.bind_group = Some(bind_group);
        self.compute_pipeline = Some(pipeline);
    }

    fn compile_and_create_compute_module(&self, compute_shader_src: &str) -> Option<wgpu::ShaderModule> {
        // WebGPU wants its shaders pre-compiled in binary SPIR-V format.
        // So we'll take the source code of our compute shader and compile it
        // with the help of the shaderc crate.
        let mut compiler = shaderc::Compiler::new().unwrap();
        let cs_spirv = compiler
            .compile_into_spirv(
                compute_shader_src,
                shaderc::ShaderKind::Compute,
                "Compute shader",
                "main",
                None,
            )
            .unwrap();
        let cs_data = wgpu::util::make_spirv(cs_spirv.as_binary_u8());

        // Now with the binary data we can create and return our ShaderModule,
        // which will be executed on the GPU within our compute pipeline.
        Some(
            self.device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: None,
                source: cs_data,
                flags: wgpu::ShaderFlags::default(),
            })
        )
    }

    fn create_compute_pipeline(&self, cs_module: &wgpu::ShaderModule) -> (wgpu::BindGroup, wgpu::ComputePipeline) {
        // Setup bind groups
        let mut group_layout_entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
        let mut group_entries: Vec<wgpu::BindGroupEntry> = Vec::new();

        // TODO: just assumes that all layouts are COMPUTE + rw STORAGE + ...
        for i in 0..self.buffer_bindings.len() {
            let b = self.buffer_bindings[i];

            group_layout_entries.push(
                wgpu::BindGroupLayoutEntry {
                    binding: b,
                    visibility: wgpu::ShaderStage::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            );

            group_entries.push(
                wgpu::BindGroupEntry {
                    binding: b,
                    resource: self.upload_buffers.get(i).unwrap().as_entire_binding(),
                }
            );
        }

        let bind_group_layout = self.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &group_layout_entries,
            }
        );

        let bind_group = self.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &group_entries,
            }
        );

        // Setup pipeline
        let compute_pipeline_layout = self.device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            }
        );

        let compute_pipeline = self.device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&compute_pipeline_layout),
                module: &cs_module,
                entry_point: "main",
            }
        );

        (bind_group, compute_pipeline)
    }

    /// Launches compute work groups; `x`, `y`, `z` many in their respective dimensions.
    /// To launch a 1D or 2D work group, set the unwanted dimension to 1.
    /// (Work groups in GLSL are the same thing as blocks in CUDA. The equivalent of CUDA threads
    ///  in GLSL are called invocations. These are defined in the shaders themselves.)
    pub fn compute(&mut self, x: u32, y: u32, z: u32) {
        // Use a CommandEncoder to batch all commands that you wish to send to the GPU to execute.
        // The resulting CommandBuffer can then be submitted to the GPU via a Queue.
        // Signal the end of the batch with CommandEncoder#finish().
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            // The compute pass will start ("dispatch") our compute shader.
            let mut compute_pass =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
            compute_pass.set_pipeline(self.compute_pipeline.as_ref().unwrap());
            compute_pass.set_bind_group(0, self.bind_group.as_ref().unwrap(), &[]);
            compute_pass.insert_debug_marker("Pasture Compute Debug");
            compute_pass.dispatch(x, y, z);
        }

        // Copy buffers
        {
            for i in 0..self.upload_buffers.len() {
                let upload = self.upload_buffers.get(i).unwrap();
                let download = self.download_buffers.get(i).unwrap();
                let size = self.buffer_sizes.get(i).unwrap();

                encoder.copy_buffer_to_buffer(upload, 0, download, 0, *size);
            }
        }

        // Submit to queue
        self.queue.submit(Some(encoder.finish()));
    }
}

// == Helper types ===============================================================================

/// Defines the desired capabilities of a device that is to be retrieved.
// TODO: be more flexible about features and limits
pub struct DeviceOptions {
    pub device_power: DevicePower,
    pub device_backend: DeviceBackend,
    pub use_adapter_features: bool,
    pub use_adapter_limits: bool,
}

impl Default for DeviceOptions {
    fn default() -> Self {
        Self {
            device_power: DevicePower::Low,
            device_backend: DeviceBackend::Primary,
            use_adapter_features: false,
            use_adapter_limits: false,
        }
    }
}

pub enum DevicePower {
    /// Usually an integrated GPU
    Low = 0,
    /// Usually a discrete GPU
    High = 1,
}

impl Default for DevicePower {
    /// Default is [DevicePower::Low]
    fn default() -> Self { Self::Low }
}

pub enum DeviceBackend {
    /// Primary backends for wgpu: Vulkan, Metal, Dx12, Browser
    Primary,
    /// Secondary backends for wgpu: OpenGL, Dx11
    Secondary,
    Vulkan,
    Metal,
    Dx12,
    Dx11,
    OpenGL,
    Browser,
}

impl Default for DeviceBackend {
    /// Default is [DeviceBackend::Primary]
    fn default() -> Self { Self::Primary }
}

/// Associates a point buffer attribute with a binding defined in a (compute) shader.
// TODO: consider usage, size, mapped_at_creation, type (SSBO vs UBO), etc.
pub struct BufferInfo<'a> {
    pub attribute: &'a layout::PointAttributeDefinition,
    pub binding: u32,
}