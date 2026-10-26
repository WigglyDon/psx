//! Narrow frontend for the retained libretro Vulkan contract. Guest rendering
//! remains native; only completed RGBA bytes cross into the existing GLES UI.
//! ABI derives from the retained MIT libretro_vulkan.h (RetroArch team).
use ash::{vk, Device, Entry, Instance};
use libloading::Library;
use std::{
    ffi::{c_char, c_void},
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
};

type Result<T> = std::result::Result<T, String>;
const MAX_WIDTH: u32 = 1024;
const MAX_HEIGHT: u32 = 1024;
const COPY_WAIT_NS: u64 = 1_000_000_000;

#[derive(Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Image {
    pub view: vk::ImageView,
    pub layout: vk::ImageLayout,
    pub info: vk::ImageViewCreateInfo,
}
// The donor owns the image until our synchronous completion acknowledgement.
// These are opaque handles/metadata, not Rust references; all access is locked.
unsafe impl Send for Image {}

#[repr(C)]
#[derive(Default)]
pub(crate) struct Context {
    gpu: vk::PhysicalDevice,
    device: vk::Device,
    queue: vk::Queue,
    family: u32,
    presentation_queue: vk::Queue,
    presentation_family: u32,
}
type CreateDevice = unsafe extern "C" fn(
    *mut Context,
    vk::Instance,
    vk::PhysicalDevice,
    vk::SurfaceKHR,
    vk::PFN_vkGetInstanceProcAddr,
    *const *const c_char,
    u32,
    *const *const c_char,
    u32,
    *const vk::PhysicalDeviceFeatures,
) -> bool;
// Only the v1 prefix is read, even when the donor advertises v2.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Negotiation {
    pub kind: u32,
    pub version: u32,
    pub application: Option<unsafe extern "C" fn() -> *const vk::ApplicationInfo>,
    pub create: Option<CreateDevice>,
    pub destroy: Option<unsafe extern "C" fn()>,
}

#[repr(C)]
pub(crate) struct Interface {
    kind: u32,
    version: u32,
    handle: *mut c_void,
    instance: vk::Instance,
    gpu: vk::PhysicalDevice,
    device: vk::Device,
    get_device_proc_address: vk::PFN_vkGetDeviceProcAddr,
    get_instance_proc_address: vk::PFN_vkGetInstanceProcAddr,
    queue: vk::Queue,
    family: u32,
    set_image: unsafe extern "C" fn(*mut c_void, *const Image, u32, *const vk::Semaphore, u32),
    sync_index: unsafe extern "C" fn(*mut c_void) -> u32,
    sync_mask: unsafe extern "C" fn(*mut c_void) -> u32,
    command_buffers: unsafe extern "C" fn(*mut c_void, u32, *const vk::CommandBuffer),
    wait: unsafe extern "C" fn(*mut c_void),
    lock: unsafe extern "C" fn(*mut c_void),
    unlock: unsafe extern "C" fn(*mut c_void),
    signal: unsafe extern "C" fn(*mut c_void, vk::Semaphore),
}
// Immutable interface, stable Arc address; its pointee uses separate locks.
unsafe impl Send for Interface {}
unsafe impl Sync for Interface {}

#[derive(Default)]
struct QueueLock(Mutex<bool>, Condvar);
impl QueueLock {
    fn lock(&self) {
        let mut busy = self.0.lock().unwrap_or_else(|e| e.into_inner());
        while *busy {
            busy = self.1.wait(busy).unwrap_or_else(|e| e.into_inner());
        }
        *busy = true;
    }
    fn unlock(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.1.notify_one();
    }
}

#[derive(Default)]
pub(crate) struct Frontend {
    negotiation: Mutex<Option<Negotiation>>,
    bridge: Mutex<Option<Bridge>>,
    interface: OnceLock<Interface>,
    queue: QueueLock,
    error: Mutex<Option<String>>,
    completed: AtomicBool,
}
impl Frontend {
    pub unsafe fn negotiate(&self, data: *const c_void) -> bool {
        let n = *(data.cast::<Negotiation>());
        if n.kind != 0 || !(1..=2).contains(&n.version) || n.create.is_none() || n.destroy.is_some()
        {
            return false;
        }
        *self.negotiation.lock().unwrap() = Some(n);
        true
    }
    pub unsafe fn initialize(self: &Arc<Self>) -> Result<()> {
        let n = self
            .negotiation
            .lock()
            .unwrap()
            .ok_or("PS2 did not negotiate Vulkan.")?;
        let bridge = Bridge::begin(n.application)?;
        let instance = bridge.instance.handle();
        let gpu = bridge.gpu;
        let proc_address = bridge.entry.static_fn().get_instance_proc_addr;
        // Retain even a partially initialized instance until native GS teardown.
        // Never hold the state lock while calling the donor/its callbacks.
        *self.bridge.lock().unwrap() = Some(bridge);
        let mut context = Context::default();
        if !n.create.unwrap()(
            &mut context,
            instance,
            gpu,
            vk::SurfaceKHR::null(),
            proc_address,
            ptr::null(),
            0,
            ptr::null(),
            0,
            &vk::PhysicalDeviceFeatures::default(),
        ) {
            return Err("The PS2 Vulkan renderer could not create its device.".into());
        }
        let mut state = self.bridge.lock().unwrap();
        let bridge = state.as_mut().unwrap();
        // Retain the returned device for cleanup even if its queue contract fails.
        if context.device != vk::Device::null() {
            bridge.device = Some(Device::load(bridge.instance.fp_v1_0(), context.device));
        }
        let families = bridge
            .instance
            .get_physical_device_queue_family_properties(context.gpu);
        if context.gpu != gpu
            || context.device == vk::Device::null()
            || context.queue == vk::Queue::null()
            || context.presentation_queue != context.queue
            || context.presentation_family != context.family
            || !families
                .get(context.family as usize)
                .is_some_and(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        {
            return Err("The PS2 renderer returned an unsupported Vulkan queue.".into());
        }
        bridge.queue = context.queue;
        bridge.family = context.family;
        self.interface
            .set(Interface {
                kind: 0,
                version: 5,
                handle: Arc::as_ptr(self).cast_mut().cast(),
                instance,
                gpu,
                device: context.device,
                get_device_proc_address: bridge.instance.fp_v1_0().get_device_proc_addr,
                get_instance_proc_address: proc_address,
                queue: context.queue,
                family: context.family,
                set_image,
                sync_index,
                sync_mask,
                command_buffers,
                wait,
                lock,
                unlock,
                signal,
            })
            .map_err(|_| "PS2 Vulkan was initialized twice.")?;
        bridge.prepare()?;
        Ok(())
    }
    pub fn interface(&self) -> Option<*const Interface> {
        self.interface.get().map(ptr::from_ref)
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
    fn fail(&self, error: String) {
        self.completed.store(false, Ordering::Release);
        let mut first = self.error.lock().unwrap();
        if first.is_none() {
            *first = Some(error);
        }
    }
    pub fn acknowledge(&self) -> bool {
        self.error().is_none() && self.completed.swap(false, Ordering::AcqRel)
    }
    pub unsafe fn read(&self, width: u32, height: u32) -> Result<Pixels> {
        if let Some(e) = self.error() {
            return Err(e);
        }
        let result = self
            .bridge
            .lock()
            .unwrap()
            .as_mut()
            .ok_or("PS2 Vulkan is not ready.".into())
            .and_then(|b| b.read(&self.queue, width, height));
        match result {
            Ok(pixels) => {
                self.completed.store(true, Ordering::Release);
                Ok(pixels)
            }
            Err(e) => {
                self.fail(e.clone());
                Err(e)
            }
        }
    }
    // Call only after the native CPU/GS owners and callbacks have joined.
    pub fn retire(&self) {
        self.bridge.lock().unwrap().take();
    }
}

unsafe fn frontend<'a>(handle: *mut c_void) -> &'a Frontend {
    &*handle.cast()
}
unsafe extern "C" fn set_image(
    handle: *mut c_void,
    image: *const Image,
    count: u32,
    _sems: *const vk::Semaphore,
    family: u32,
) {
    let f = frontend(handle);
    f.completed.store(false, Ordering::Release);
    if f.error().is_some() {
        return;
    }
    let mut state = f.bridge.lock().unwrap();
    let Some(b) = state.as_mut() else {
        f.fail("PS2 Vulkan image arrived before device setup.".into());
        return;
    };
    if image.is_null() {
        b.image = None;
        return;
    }
    if count != 0 || (family != b.family && family != vk::QUEUE_FAMILY_IGNORED) || b.image.is_some()
    {
        f.fail("The PS2 Vulkan image handoff violated its queue/ownership contract.".into());
        return;
    }
    b.image = Some(*image);
}
unsafe extern "C" fn sync_index(_: *mut c_void) -> u32 {
    0
}
unsafe extern "C" fn sync_mask(_: *mut c_void) -> u32 {
    1
}
unsafe extern "C" fn command_buffers(handle: *mut c_void, _: u32, _: *const vk::CommandBuffer) {
    frontend(handle).fail("The PS2 renderer requested an unsupported deferred submission.".into());
}
unsafe extern "C" fn signal(handle: *mut c_void, semaphore: vk::Semaphore) {
    if semaphore != vk::Semaphore::null() {
        frontend(handle).fail("The PS2 renderer requested an unsupported signal semaphore.".into());
    }
}
unsafe extern "C" fn lock(handle: *mut c_void) {
    frontend(handle).queue.lock();
}
unsafe extern "C" fn unlock(handle: *mut c_void) {
    frontend(handle).queue.unlock();
}
unsafe extern "C" fn wait(handle: *mut c_void) {
    let f = frontend(handle);
    if let Some(b) = f.bridge.lock().unwrap().as_mut() {
        if let Err(e) = b.finish() {
            f.fail(e);
        }
    }
}

fn extent(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err("The PS2 picture exceeded its native-resolution buffer.".into());
    }
    Ok(width as usize * height as usize * 4)
}
fn check_image(image: &Image) -> Result<bool> {
    let range = image.info.subresource_range;
    if image.info.s_type != vk::StructureType::IMAGE_VIEW_CREATE_INFO
        || !image.info.p_next.is_null()
        || image.info.image == vk::Image::null()
        || image.view == vk::ImageView::null()
        || image.info.view_type != vk::ImageViewType::TYPE_2D
        || range.aspect_mask != vk::ImageAspectFlags::COLOR
        || range.base_mip_level != 0
        || range.level_count != 1
        || range.base_array_layer != 0
        || range.layer_count != 1
        || ![
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ]
        .contains(&image.layout)
        || [
            image.info.components.r,
            image.info.components.g,
            image.info.components.b,
            image.info.components.a,
        ]
        .iter()
        .any(|s| *s != vk::ComponentSwizzle::IDENTITY)
    {
        return Err("The PS2 renderer returned an unsupported image view.".into());
    }
    match image.info.format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => Ok(false),
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => Ok(true),
        _ => Err("The PS2 renderer returned an unsupported image format.".into()),
    }
}
fn vk_error(operation: &str, result: vk::Result) -> String {
    format!("PS2 Vulkan {operation}: {result:?}")
}

struct Bridge {
    // Keep the system loader open until after every native and Rust dispatch.
    loader: Option<Library>,
    entry: Entry,
    instance: Instance,
    gpu: vk::PhysicalDevice,
    device: Option<Device>,
    queue: vk::Queue,
    family: u32,
    image: Option<Image>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    pending: bool,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    capacity: usize,
    coherent: bool,
    pixels: Vec<Arc<Vec<u8>>>,
}
impl Bridge {
    unsafe fn begin(
        application: Option<unsafe extern "C" fn() -> *const vk::ApplicationInfo>,
    ) -> Result<Self> {
        let loader = Library::new(if cfg!(target_os = "android") {
            "libvulkan.so"
        } else {
            "libvulkan.so.1"
        })
        .map_err(|e| format!("The system Vulkan renderer is unavailable: {e}"))?;
        let get_instance_proc_addr = *loader
            .get::<vk::PFN_vkGetInstanceProcAddr>(b"vkGetInstanceProcAddr\0")
            .map_err(|e| e.to_string())?;
        let entry = Entry::from_static_fn(vk::StaticFn {
            get_instance_proc_addr,
        });
        let default = vk::ApplicationInfo::builder()
            .api_version(vk::API_VERSION_1_1)
            .build();
        let app = application
            .map(|f| f())
            .filter(|p| !p.is_null())
            .unwrap_or(&default);
        if (*app).api_version > vk::API_VERSION_1_1 {
            return Err("The PS2 Vulkan API contract changed.".into());
        }
        let instance = entry
            .create_instance(
                &vk::InstanceCreateInfo::builder().application_info(&*app),
                None,
            )
            .map_err(|e| vk_error("instance creation", e))?;
        let mut bridge = Self {
            loader: Some(loader),
            entry,
            instance,
            gpu: vk::PhysicalDevice::null(),
            device: None,
            queue: vk::Queue::null(),
            family: 0,
            image: None,
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            pending: false,
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            capacity: 0,
            coherent: false,
            pixels: Vec::new(),
        };
        bridge.gpu = bridge
            .instance
            .enumerate_physical_devices()
            .map_err(|e| vk_error("device discovery", e))?
            .into_iter()
            .find(|gpu| {
                bridge
                    .instance
                    .get_physical_device_properties(*gpu)
                    .api_version
                    >= vk::API_VERSION_1_1
            })
            .ok_or("PS2 needs a Vulkan 1.1 graphics device.")?;
        Ok(bridge)
    }
    unsafe fn prepare(&mut self) -> Result<()> {
        let device = self.device.as_ref().unwrap();
        self.pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(self.family),
                None,
            )
            .map_err(|e| vk_error("copy command pool", e))?;
        self.command = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_pool(self.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .map_err(|e| vk_error("copy command buffer", e))?[0];
        self.fence = device
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .map_err(|e| vk_error("copy fence", e))?;
        Ok(())
    }
    unsafe fn finish(&mut self) -> Result<()> {
        if self.pending {
            self.device
                .as_ref()
                .unwrap()
                .wait_for_fences(&[self.fence], true, COPY_WAIT_NS)
                .map_err(|e| vk_error("frame completion", e))?;
            self.pending = false;
        }
        Ok(())
    }
    unsafe fn resize(&mut self, bytes: usize) -> Result<()> {
        if bytes <= self.capacity {
            return Ok(());
        }
        self.finish()?;
        let device = self.device.as_ref().unwrap();
        device.destroy_buffer(self.buffer, None);
        device.free_memory(self.memory, None);
        self.buffer = vk::Buffer::null();
        self.memory = vk::DeviceMemory::null();
        self.capacity = 0;
        self.buffer = device
            .create_buffer(
                &vk::BufferCreateInfo::builder()
                    .size(bytes as u64)
                    .usage(vk::BufferUsageFlags::TRANSFER_DST)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
            .map_err(|e| vk_error("staging buffer", e))?;
        let required = device.get_buffer_memory_requirements(self.buffer);
        let properties = self
            .instance
            .get_physical_device_memory_properties(self.gpu);
        let index = (0..properties.memory_type_count)
            .filter(|i| required.memory_type_bits & (1 << i) != 0)
            .filter(|i| {
                properties.memory_types[*i as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
            })
            .max_by_key(|i| {
                properties.memory_types[*i as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
            })
            .ok_or("PS2 Vulkan has no host-visible staging memory.")?;
        self.coherent = properties.memory_types[index as usize]
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);
        self.memory = device
            .allocate_memory(
                &vk::MemoryAllocateInfo::builder()
                    .allocation_size(required.size)
                    .memory_type_index(index),
                None,
            )
            .map_err(|e| vk_error("staging allocation", e))?;
        device
            .bind_buffer_memory(self.buffer, self.memory, 0)
            .map_err(|e| vk_error("staging binding", e))?;
        self.capacity = bytes;
        Ok(())
    }
    unsafe fn read(&mut self, queue: &QueueLock, width: u32, height: u32) -> Result<Pixels> {
        let bytes = extent(width, height)?;
        let image = self
            .image
            .take()
            .ok_or("The PS2 frame has no owned Vulkan image.")?;
        let bgra = check_image(&image)?;
        self.resize(bytes)?;
        self.finish()?;
        let device = self.device.as_ref().unwrap();
        device
            .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
            .map_err(|e| vk_error("copy reset", e))?;
        device
            .reset_fences(&[self.fence])
            .map_err(|e| vk_error("fence reset", e))?;
        device
            .begin_command_buffer(
                self.command,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .map_err(|e| vk_error("copy begin", e))?;
        let to_copy = vk::ImageMemoryBarrier::builder()
            .image(image.info.image)
            .old_layout(image.layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(image.info.subresource_range)
            .build();
        device.cmd_pipeline_barrier(
            self.command,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_copy],
        );
        let copy = vk::BufferImageCopy::builder()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .build();
        device.cmd_copy_image_to_buffer(
            self.command,
            image.info.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            self.buffer,
            &[copy],
        );
        let restore = vk::ImageMemoryBarrier::builder()
            .image(image.info.image)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(image.layout)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(image.info.subresource_range)
            .build();
        let host = vk::BufferMemoryBarrier::builder()
            .buffer(self.buffer)
            .offset(0)
            .size(bytes as u64)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .build();
        device.cmd_pipeline_barrier(
            self.command,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::ALL_COMMANDS | vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[],
            &[host],
            &[restore],
        );
        device
            .end_command_buffer(self.command)
            .map_err(|e| vk_error("copy end", e))?;
        let commands = [self.command];
        let submit = vk::SubmitInfo::builder().command_buffers(&commands).build();
        queue.lock();
        let submitted = device.queue_submit(self.queue, &[submit], self.fence);
        queue.unlock();
        submitted.map_err(|e| vk_error("copy submission", e))?;
        self.pending = true;
        self.finish()?;
        let device = self.device.as_ref().unwrap();
        let mapped = device
            .map_memory(self.memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
            .map_err(|e| vk_error("staging mapping", e))?;
        if !self.coherent {
            if let Err(e) =
                device.invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange::builder()
                    .memory(self.memory)
                    .offset(0)
                    .size(vk::WHOLE_SIZE)
                    .build()])
            {
                device.unmap_memory(self.memory);
                return Err(vk_error("staging visibility", e));
            }
        }
        let slot = if let Some(i) = self.pixels.iter().position(|p| Arc::strong_count(p) == 1) {
            i
        } else if self.pixels.len() < 3 {
            self.pixels.push(Arc::new(Vec::new()));
            self.pixels.len() - 1
        } else {
            device.unmap_memory(self.memory);
            return Err("The PS2 frame consumer retained too many pictures.".into());
        };
        let pixels = Arc::get_mut(&mut self.pixels[slot]).unwrap();
        pixels.resize(bytes, 0);
        ptr::copy_nonoverlapping(mapped.cast::<u8>(), pixels.as_mut_ptr(), bytes);
        device.unmap_memory(self.memory);
        if bgra {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Ok(Pixels {
            width,
            height,
            rgba: self.pixels[slot].clone(),
        })
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        unsafe {
            if self.finish().is_err() {
                // A timed-out command may still own its resources. Never destroy
                // them or unload dispatch underneath it. The failed app process
                // must retire; native/host fault evidence remains available.
                if let Some(loader) = self.loader.take() {
                    std::mem::forget(loader);
                }
                return;
            }
            if let Some(device) = &self.device {
                device.destroy_buffer(self.buffer, None);
                device.free_memory(self.memory, None);
                device.destroy_fence(self.fence, None);
                device.destroy_command_pool(self.pool, None);
                device.destroy_device(None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_bounds_are_checked_before_any_driver_call() {
        assert_eq!(extent(640, 448).unwrap(), 640 * 448 * 4);
        for (w, h) in [
            (0, 448),
            (640, 0),
            (1025, 1),
            (1, u32::MAX),
            (u32::MAX, u32::MAX),
        ] {
            assert!(extent(w, h).is_err());
        }
    }
    #[test]
    fn failed_or_duplicate_reads_never_release_the_producer() {
        let f = Frontend::default();
        assert!(!f.acknowledge());
        f.completed.store(true, Ordering::Release);
        assert!(f.acknowledge());
        assert!(!f.acknowledge());
        f.completed.store(true, Ordering::Release);
        f.fail("first".into());
        f.fail("second".into());
        assert_eq!(f.error().as_deref(), Some("first"));
        assert!(!f.acknowledge());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn layouts_match_the_retained_c_header() {
        use std::{
            mem::{offset_of, size_of},
            process::Command,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = std::env::temp_dir().join(format!("psx-vulkan-abi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abi.c"), r#"
#define VK_NO_PROTOTYPES
#include <libretro_vulkan.h>
#include <stddef.h>
#include <stdio.h>
int main(void) {
printf("%zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu\n",
sizeof(struct retro_vulkan_image), offsetof(struct retro_vulkan_image,create_info),
sizeof(struct retro_vulkan_context), offsetof(struct retro_vulkan_context,presentation_queue),
offsetof(struct retro_hw_render_context_negotiation_interface_vulkan,create_instance),
offsetof(struct retro_hw_render_context_negotiation_interface_vulkan,create_device),
sizeof(struct retro_hw_render_interface_vulkan), offsetof(struct retro_hw_render_interface_vulkan,queue),
offsetof(struct retro_hw_render_interface_vulkan,set_image), offsetof(struct retro_hw_render_interface_vulkan,lock_queue),
offsetof(struct retro_hw_render_interface_vulkan,set_signal_semaphore),sizeof(bool));
return 0;
}
"#).unwrap();
        let compiled = Command::new("cc")
            .arg(dir.join("abi.c"))
            .arg("-I")
            .arg(root.join("vendor/armsx2/3rdparty/libretro"))
            .arg("-I")
            .arg(root.join("vendor/armsx2/3rdparty/vulkan/include"))
            .arg("-o")
            .arg(dir.join("abi"))
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let output = Command::new(dir.join("abi")).output().unwrap();
        assert!(output.status.success());
        let actual: Vec<usize> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(
            actual,
            [
                size_of::<Image>(),
                offset_of!(Image, info),
                size_of::<Context>(),
                offset_of!(Context, presentation_queue),
                size_of::<Negotiation>(),
                offset_of!(Negotiation, create),
                size_of::<Interface>(),
                offset_of!(Interface, queue),
                offset_of!(Interface, set_image),
                offset_of!(Interface, lock),
                offset_of!(Interface, signal),
                size_of::<bool>()
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Runs only in the prepared software-Vulkan host campaign. No phone or
    /// host display is acquired. Exercise real Vulkan commands/readback, while
    /// keeping native GS/gameplay acceptance separate.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires the prepared CPU-only lavapipe campaign"]
    fn software_vulkan_negotiates_and_copies_real_completed_images() {
        assert_eq!(
            std::env::var("PSX_SOFTWARE_VULKAN_TEST").as_deref(),
            Ok("1")
        );
        assert!(std::env::var("VK_ICD_FILENAMES")
            .unwrap()
            .ends_with("lvp_icd.x86_64.json"));
        unsafe extern "C" fn create(
            out: *mut Context,
            instance: vk::Instance,
            gpu: vk::PhysicalDevice,
            _: vk::SurfaceKHR,
            proc_address: vk::PFN_vkGetInstanceProcAddr,
            _: *const *const c_char,
            _: u32,
            _: *const *const c_char,
            _: u32,
            _: *const vk::PhysicalDeviceFeatures,
        ) -> bool {
            let entry = Entry::from_static_fn(vk::StaticFn {
                get_instance_proc_addr: proc_address,
            });
            let instance = Instance::load(entry.static_fn(), instance);
            let family = instance
                .get_physical_device_queue_family_properties(gpu)
                .iter()
                .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .unwrap() as u32;
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(family)
                .queue_priorities(&priorities)
                .build()];
            let device = instance
                .create_device(
                    gpu,
                    &vk::DeviceCreateInfo::builder().queue_create_infos(&queues),
                    None,
                )
                .unwrap();
            let queue = device.get_device_queue(family, 0);
            *out = Context {
                gpu,
                device: device.handle(),
                queue,
                family,
                presentation_queue: queue,
                presentation_family: family,
            };
            true
        }
        unsafe {
            let frontend = Arc::new(Frontend::default());
            let negotiation = Negotiation {
                kind: 0,
                version: 1,
                application: None,
                create: Some(create),
                destroy: None,
            };
            assert!(frontend.negotiate(ptr::from_ref(&negotiation).cast()));
            frontend.initialize().unwrap();
            let interface = frontend.interface().unwrap();
            assert_eq!(((*interface).sync_index)((*interface).handle), 0);
            assert_eq!(((*interface).sync_mask)((*interface).handle), 1);
            for format in [vk::Format::R8G8B8A8_UNORM, vk::Format::B8G8R8A8_UNORM] {
                let (image, view, memory) = {
                    let mut state = frontend.bridge.lock().unwrap();
                    let b = state.as_mut().unwrap();
                    let device = b.device.as_ref().unwrap();
                    let image = device
                        .create_image(
                            &vk::ImageCreateInfo::builder()
                                .image_type(vk::ImageType::TYPE_2D)
                                .format(format)
                                .extent(vk::Extent3D {
                                    width: 4,
                                    height: 2,
                                    depth: 1,
                                })
                                .mip_levels(1)
                                .array_layers(1)
                                .samples(vk::SampleCountFlags::TYPE_1)
                                .tiling(vk::ImageTiling::OPTIMAL)
                                .usage(
                                    vk::ImageUsageFlags::TRANSFER_SRC
                                        | vk::ImageUsageFlags::TRANSFER_DST
                                        | vk::ImageUsageFlags::SAMPLED,
                                )
                                .sharing_mode(vk::SharingMode::EXCLUSIVE),
                            None,
                        )
                        .unwrap();
                    let required = device.get_image_memory_requirements(image);
                    let index = required.memory_type_bits.trailing_zeros();
                    let memory = device
                        .allocate_memory(
                            &vk::MemoryAllocateInfo::builder()
                                .allocation_size(required.size)
                                .memory_type_index(index),
                            None,
                        )
                        .unwrap();
                    device.bind_image_memory(image, memory, 0).unwrap();
                    let range = vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    };
                    let view = device
                        .create_image_view(
                            &vk::ImageViewCreateInfo::builder()
                                .image(image)
                                .view_type(vk::ImageViewType::TYPE_2D)
                                .format(format)
                                .subresource_range(range),
                            None,
                        )
                        .unwrap();
                    device
                        .reset_command_pool(b.pool, vk::CommandPoolResetFlags::empty())
                        .unwrap();
                    device
                        .begin_command_buffer(b.command, &vk::CommandBufferBeginInfo::default())
                        .unwrap();
                    let start = vk::ImageMemoryBarrier::builder()
                        .image(image)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .subresource_range(range)
                        .build();
                    device.cmd_pipeline_barrier(
                        b.command,
                        vk::PipelineStageFlags::TOP_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[start],
                    );
                    device.cmd_clear_color_image(
                        b.command,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue {
                            float32: [0.2, 0.4, 0.6, 1.],
                        },
                        &[range],
                    );
                    let end = vk::ImageMemoryBarrier::builder()
                        .image(image)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .subresource_range(range)
                        .build();
                    device.cmd_pipeline_barrier(
                        b.command,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[end],
                    );
                    device.end_command_buffer(b.command).unwrap();
                    device.reset_fences(&[b.fence]).unwrap();
                    device
                        .queue_submit(
                            b.queue,
                            &[vk::SubmitInfo::builder()
                                .command_buffers(&[b.command])
                                .build()],
                            b.fence,
                        )
                        .unwrap();
                    b.pending = true;
                    b.finish().unwrap();
                    (image, view, memory)
                };
                let value = Image {
                    view,
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    info: vk::ImageViewCreateInfo::builder()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .build(),
                };
                let mut retained = None;
                for _ in 0..3 {
                    set_image(
                        (*interface).handle,
                        &value,
                        0,
                        ptr::null(),
                        (*interface).family,
                    );
                    assert!(!frontend.acknowledge());
                    let pixels = frontend.read(4, 2).unwrap();
                    assert_eq!(pixels.rgba.len(), 32);
                    for pixel in pixels.rgba.chunks_exact(4) {
                        assert_eq!(pixel, &[51, 102, 153, 255]);
                    }
                    assert!(frontend.acknowledge());
                    assert!(!frontend.acknowledge());
                    if let Some(previous) = &retained {
                        assert_eq!(pixels.rgba, *previous);
                    }
                    retained = Some(pixels.rgba);
                }
                let state = frontend.bridge.lock().unwrap();
                let b = state.as_ref().unwrap();
                let device = b.device.as_ref().unwrap();
                device.destroy_image_view(view, None);
                device.destroy_image(image, None);
                device.free_memory(memory, None);
            }
            frontend.retire();
        }
    }
}
