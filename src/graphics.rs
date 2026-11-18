use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use ash::vk::Handle;
use glow::HasContext;
use sdl2::event::{Event, WindowEvent};
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{EventPump, Sdl, VideoSubsystem};

use crate::error::Error;
use crate::value::Value;

thread_local! {
    static OPENGL: RefCell<Option<GlState>> = const { RefCell::new(None) };
    static VULKAN: RefCell<Option<VkState>> = const { RefCell::new(None) };
}

pub fn call_gl(method: &str, args: &[Value], line: usize) -> Result<Value, Error> {
    match method {
        "init" => gl_init(args, line),
        "frame" => {
            expect_arity("gl.frame", args, 3, 4, line)?;
            let red = float_arg("gl.frame", &args[0], line)? as f32;
            let green = float_arg("gl.frame", &args[1], line)? as f32;
            let blue = float_arg("gl.frame", &args[2], line)? as f32;
            let alpha = if args.len() == 4 {
                float_arg("gl.frame", &args[3], line)? as f32
            } else {
                1.0
            };
            with_gl(line, |state| unsafe {
                if !state.poll() {
                    return Ok(Value::Bool(false));
                }
                state.gl.clear_color(red, green, blue, alpha);
                state.gl.clear(glow::COLOR_BUFFER_BIT);
                state.window.gl_swap_window();
                Ok(Value::Bool(true))
            })
        }
        "close" | "shutdown" => {
            expect_arity("gl.close", args, 0, 0, line)?;
            with_gl(line, |_| Ok(Value::None))?;
            OPENGL.with(|cell| *cell.borrow_mut() = None);
            Ok(Value::None)
        }
        "poll" => {
            expect_arity("gl.poll", args, 0, 0, line)?;
            with_gl(line, |state| Ok(Value::Bool(state.poll())))
        }
        "shouldClose" | "should_close" => {
            expect_arity("gl.shouldClose", args, 0, 0, line)?;
            with_gl(line, |state| Ok(Value::Bool(!state.running)))
        }
        "present" | "swap" => {
            expect_arity("gl.present", args, 0, 0, line)?;
            with_gl(line, |state| {
                state.window.gl_swap_window();
                Ok(Value::None)
            })
        }
        "setTitle" | "set_title" => {
            expect_arity("gl.setTitle", args, 1, 1, line)?;
            let title = text_arg("gl.setTitle", &args[0], line)?;
            with_gl(line, |state| {
                state
                    .window
                    .set_title(&title)
                    .map_err(|message| Error::new(line, format!("gl.setTitle failed: {message}")))?;
                Ok(Value::None)
            })
        }
        "size" => {
            expect_arity("gl.size", args, 0, 0, line)?;
            with_gl(line, |state| {
                let (width, height) = state.window.size();
                Ok(Value::list(vec![
                    Value::Int(width as i64),
                    Value::Int(height as i64),
                ]))
            })
        }
        "clearColor" | "clear_color" => {
            expect_arity("gl.clearColor", args, 3, 4, line)?;
            let red = float_arg("gl.clearColor", &args[0], line)?;
            let green = float_arg("gl.clearColor", &args[1], line)?;
            let blue = float_arg("gl.clearColor", &args[2], line)?;
            let alpha = if args.len() == 4 {
                float_arg("gl.clearColor", &args[3], line)?
            } else {
                1.0
            };
            with_gl(line, |state| unsafe {
                state.gl.clear_color(red as f32, green as f32, blue as f32, alpha as f32);
                Ok(Value::None)
            })
        }
        "clear" => {
            expect_arity("gl.clear", args, 0, 1, line)?;
            let mask = if args.is_empty() {
                glow::COLOR_BUFFER_BIT
            } else {
                clear_mask(&text_arg("gl.clear", &args[0], line)?, line)?
            };
            with_gl(line, |state| unsafe {
                state.gl.clear(mask);
                Ok(Value::None)
            })
        }
        "clearDepth" | "clear_depth" => {
            expect_arity("gl.clearDepth", args, 1, 1, line)?;
            let depth = float_arg("gl.clearDepth", &args[0], line)?;
            with_gl(line, |state| unsafe {
                state.gl.clear_depth(depth);
                Ok(Value::None)
            })
        }
        "viewport" => {
            expect_arity("gl.viewport", args, 4, 4, line)?;
            let x = int_arg("gl.viewport", &args[0], line)?;
            let y = int_arg("gl.viewport", &args[1], line)?;
            let width = int_arg("gl.viewport", &args[2], line)?;
            let height = int_arg("gl.viewport", &args[3], line)?;
            with_gl(line, |state| unsafe {
                state
                    .gl
                    .viewport(x as i32, y as i32, width as i32, height as i32);
                Ok(Value::None)
            })
        }
        "getError" | "get_error" => {
            expect_arity("gl.getError", args, 0, 0, line)?;
            with_gl(line, |state| Ok(Value::Int(unsafe { state.gl.get_error() } as i64)))
        }
        "errorName" | "error_name" => {
            expect_arity("gl.errorName", args, 1, 1, line)?;
            let code = int_arg("gl.errorName", &args[0], line)? as u32;
            Ok(Value::Text(gl_error_name(code).to_string()))
        }
        "getString" | "get_string" => {
            expect_arity("gl.getString", args, 1, 1, line)?;
            let name = text_arg("gl.getString", &args[0], line)?;
            with_gl(line, |state| unsafe {
                let constant = string_constant(&name, line)?;
                Ok(Value::Text(state.gl.get_parameter_string(constant)))
            })
        }
        "enable" => {
            expect_arity("gl.enable", args, 1, 1, line)?;
            let name = text_arg("gl.enable", &args[0], line)?;
            let capability = capability_constant(&name, line)?;
            with_gl(line, |state| unsafe {
                state.gl.enable(capability);
                Ok(Value::None)
            })
        }
        "disable" => {
            expect_arity("gl.disable", args, 1, 1, line)?;
            let name = text_arg("gl.disable", &args[0], line)?;
            let capability = capability_constant(&name, line)?;
            with_gl(line, |state| unsafe {
                state.gl.disable(capability);
                Ok(Value::None)
            })
        }
        "blendFunc" | "blend_func" => {
            expect_arity("gl.blendFunc", args, 2, 2, line)?;
            let source = blend_constant(&text_arg("gl.blendFunc", &args[0], line)?, line)?;
            let destination = blend_constant(&text_arg("gl.blendFunc", &args[1], line)?, line)?;
            with_gl(line, |state| unsafe {
                state.gl.blend_func(source, destination);
                Ok(Value::None)
            })
        }
        "depthFunc" | "depth_func" => {
            expect_arity("gl.depthFunc", args, 1, 1, line)?;
            let function = depth_constant(&text_arg("gl.depthFunc", &args[0], line)?, line)?;
            with_gl(line, |state| unsafe {
                state.gl.depth_func(function);
                Ok(Value::None)
            })
        }
        "cullFace" | "cull_face" => {
            expect_arity("gl.cullFace", args, 1, 1, line)?;
            let face = face_constant(&text_arg("gl.cullFace", &args[0], line)?, line)?;
            with_gl(line, |state| unsafe {
                state.gl.cull_face(face);
                Ok(Value::None)
            })
        }
        "createShader" | "create_shader" => {
            expect_arity("gl.createShader", args, 2, 2, line)?;
            let stage = text_arg("gl.createShader", &args[0], line)?;
            let source = text_arg("gl.createShader", &args[1], line)?;
            with_gl(line, |state| state.create_shader(&stage, &source, line))
        }
        "deleteShader" | "delete_shader" => {
            expect_arity("gl.deleteShader", args, 1, 1, line)?;
            let handle = handle_arg("gl.deleteShader", &args[0], line)?;
            with_gl(line, |state| state.delete_shader(handle, line))
        }
        "createProgram" | "create_program" => {
            expect_arity("gl.createProgram", args, 2, 2, line)?;
            let vertex = handle_arg("gl.createProgram", &args[0], line)?;
            let fragment = handle_arg("gl.createProgram", &args[1], line)?;
            with_gl(line, |state| state.create_program(vertex, fragment, line))
        }
        "useProgram" | "use_program" => {
            expect_arity("gl.useProgram", args, 1, 1, line)?;
            let handle = handle_arg("gl.useProgram", &args[0], line)?;
            with_gl(line, |state| state.use_program(handle, line))
        }
        "deleteProgram" | "delete_program" => {
            expect_arity("gl.deleteProgram", args, 1, 1, line)?;
            let handle = handle_arg("gl.deleteProgram", &args[0], line)?;
            with_gl(line, |state| state.delete_program(handle, line))
        }
        "createBuffer" | "create_buffer" => {
            expect_arity("gl.createBuffer", args, 0, 0, line)?;
            with_gl(line, |state| state.create_buffer(line))
        }
        "deleteBuffer" | "delete_buffer" => {
            expect_arity("gl.deleteBuffer", args, 1, 1, line)?;
            let handle = handle_arg("gl.deleteBuffer", &args[0], line)?;
            with_gl(line, |state| state.delete_buffer(handle, line))
        }
        "bindBuffer" | "bind_buffer" => {
            expect_arity("gl.bindBuffer", args, 2, 2, line)?;
            let target = buffer_target(&text_arg("gl.bindBuffer", &args[0], line)?, line)?;
            let handle = handle_arg("gl.bindBuffer", &args[1], line)?;
            with_gl(line, |state| state.bind_buffer(target, handle, line))
        }
        "bufferData" | "buffer_data" => {
            expect_arity("gl.bufferData", args, 3, 4, line)?;
            let target = buffer_target(&text_arg("gl.bufferData", &args[0], line)?, line)?;
            let usage = buffer_usage(&text_arg("gl.bufferData", &args[2], line)?, line)?;
            let data_type = if args.len() == 4 {
                Some(text_arg("gl.bufferData", &args[3], line)?)
            } else {
                None
            };
            let bytes = buffer_bytes(&args[1], data_type.as_deref(), line)?;
            with_gl(line, |state| unsafe {
                state.gl.buffer_data_u8_slice(target, &bytes, usage);
                Ok(Value::None)
            })
        }
        "createVertexArray" | "create_vertex_array" => {
            expect_arity("gl.createVertexArray", args, 0, 0, line)?;
            with_gl(line, |state| state.create_vertex_array(line))
        }
        "deleteVertexArray" | "delete_vertex_array" => {
            expect_arity("gl.deleteVertexArray", args, 1, 1, line)?;
            let handle = handle_arg("gl.deleteVertexArray", &args[0], line)?;
            with_gl(line, |state| state.delete_vertex_array(handle, line))
        }
        "bindVertexArray" | "bind_vertex_array" => {
            expect_arity("gl.bindVertexArray", args, 1, 1, line)?;
            let handle = handle_arg("gl.bindVertexArray", &args[0], line)?;
            with_gl(line, |state| state.bind_vertex_array(handle, line))
        }
        "enableVertexAttribArray" | "enable_vertex_attrib_array" => {
            expect_arity("gl.enableVertexAttribArray", args, 1, 1, line)?;
            let index = index_arg("gl.enableVertexAttribArray", &args[0], line)?;
            with_gl(line, |state| unsafe {
                state.gl.enable_vertex_attrib_array(index);
                Ok(Value::None)
            })
        }
        "disableVertexAttribArray" | "disable_vertex_attrib_array" => {
            expect_arity("gl.disableVertexAttribArray", args, 1, 1, line)?;
            let index = index_arg("gl.disableVertexAttribArray", &args[0], line)?;
            with_gl(line, |state| unsafe {
                state.gl.disable_vertex_attrib_array(index);
                Ok(Value::None)
            })
        }
        "vertexAttribPointer" | "vertex_attrib_pointer" => {
            expect_arity("gl.vertexAttribPointer", args, 6, 6, line)?;
            let index = index_arg("gl.vertexAttribPointer", &args[0], line)?;
            let size = index_arg("gl.vertexAttribPointer", &args[1], line)?;
            let data_type = text_arg("gl.vertexAttribPointer", &args[2], line)?;
            let normalized = bool_arg("gl.vertexAttribPointer", &args[3], line)?;
            let stride = int_arg("gl.vertexAttribPointer", &args[4], line)? as i32;
            let offset = int_arg("gl.vertexAttribPointer", &args[5], line)? as i32;
            let gl_type = vertex_type(&data_type, line)?;
            with_gl(line, |state| unsafe {
                state
                    .gl
                    .vertex_attrib_pointer_f32(index, size as i32, gl_type, normalized, stride, offset);
                Ok(Value::None)
            })
        }
        "drawArrays" | "draw_arrays" => {
            expect_arity("gl.drawArrays", args, 3, 3, line)?;
            let mode = draw_mode(&text_arg("gl.drawArrays", &args[0], line)?, line)?;
            let first = int_arg("gl.drawArrays", &args[1], line)? as i32;
            let count = int_arg("gl.drawArrays", &args[2], line)? as i32;
            with_gl(line, |state| unsafe {
                state.gl.draw_arrays(mode, first, count);
                Ok(Value::None)
            })
        }
        "drawElements" | "draw_elements" => {
            expect_arity("gl.drawElements", args, 4, 4, line)?;
            let mode = draw_mode(&text_arg("gl.drawElements", &args[0], line)?, line)?;
            let count = int_arg("gl.drawElements", &args[1], line)? as i32;
            let index_type = index_type(&text_arg("gl.drawElements", &args[2], line)?, line)?;
            let offset = int_arg("gl.drawElements", &args[3], line)? as i32;
            with_gl(line, |state| unsafe {
                state.gl.draw_elements(mode, count, index_type, offset);
                Ok(Value::None)
            })
        }
        "getUniformLocation" | "get_uniform_location" => {
            expect_arity("gl.getUniformLocation", args, 2, 2, line)?;
            let program = handle_arg("gl.getUniformLocation", &args[0], line)?;
            let name = text_arg("gl.getUniformLocation", &args[1], line)?;
            with_gl(line, |state| state.get_uniform_location(program, &name, line))
        }
        "uniform1f" | "uniform_1_f" => {
            expect_arity("gl.uniform1f", args, 2, 2, line)?;
            let location = handle_arg("gl.uniform1f", &args[0], line)?;
            let value = float_arg("gl.uniform1f", &args[1], line)? as f32;
            with_gl(line, |state| state.uniform_f32(location, &[value], line))
        }
        "uniform1i" | "uniform_1_i" => {
            expect_arity("gl.uniform1i", args, 2, 2, line)?;
            let location = handle_arg("gl.uniform1i", &args[0], line)?;
            let value = int_arg("gl.uniform1i", &args[1], line)? as i32;
            with_gl(line, |state| state.uniform_i32(location, value, line))
        }
        "uniform2f" | "uniform_2_f" => {
            expect_arity("gl.uniform2f", args, 3, 3, line)?;
            let location = handle_arg("gl.uniform2f", &args[0], line)?;
            let values = [
                float_arg("gl.uniform2f", &args[1], line)? as f32,
                float_arg("gl.uniform2f", &args[2], line)? as f32,
            ];
            with_gl(line, |state| state.uniform_f32(location, &values, line))
        }
        "uniform3f" | "uniform_3_f" => {
            expect_arity("gl.uniform3f", args, 4, 4, line)?;
            let location = handle_arg("gl.uniform3f", &args[0], line)?;
            let values = [
                float_arg("gl.uniform3f", &args[1], line)? as f32,
                float_arg("gl.uniform3f", &args[2], line)? as f32,
                float_arg("gl.uniform3f", &args[3], line)? as f32,
            ];
            with_gl(line, |state| state.uniform_f32(location, &values, line))
        }
        "uniform4f" | "uniform_4_f" => {
            expect_arity("gl.uniform4f", args, 5, 5, line)?;
            let location = handle_arg("gl.uniform4f", &args[0], line)?;
            let values = [
                float_arg("gl.uniform4f", &args[1], line)? as f32,
                float_arg("gl.uniform4f", &args[2], line)? as f32,
                float_arg("gl.uniform4f", &args[3], line)? as f32,
                float_arg("gl.uniform4f", &args[4], line)? as f32,
            ];
            with_gl(line, |state| state.uniform_f32(location, &values, line))
        }
        "uniformMatrix4fv" | "uniform_matrix_4_fv" => {
            expect_arity("gl.uniformMatrix4fv", args, 2, 2, line)?;
            let location = handle_arg("gl.uniformMatrix4fv", &args[0], line)?;
            let values = matrix_values(&args[1], line)?;
            with_gl(line, |state| unsafe {
                let location = state.uniform(location, line)?;
                state
                    .gl
                    .uniform_matrix_4_f32_slice(Some(&location), false, &values);
                Ok(Value::None)
            })
        }
        other => Err(unknown_method("gl", other, line)),
    }
}

pub fn call_vk(method: &str, args: &[Value], line: usize) -> Result<Value, Error> {
    match method {
        "init" => vk_init(args, line),
        "frame" => {
            expect_arity("vk.frame", args, 3, 4, line)?;
            let red = float_arg("vk.frame", &args[0], line)? as f32;
            let green = float_arg("vk.frame", &args[1], line)? as f32;
            let blue = float_arg("vk.frame", &args[2], line)? as f32;
            let alpha = if args.len() == 4 {
                float_arg("vk.frame", &args[3], line)? as f32
            } else {
                1.0
            };
            with_vk(line, |state| {
                if !state.poll() {
                    return Ok(Value::Bool(false));
                }
                state.clear_color = [red, green, blue, alpha];
                state.present(line)?;
                Ok(Value::Bool(true))
            })
        }
        "close" | "shutdown" => {
            expect_arity("vk.close", args, 0, 0, line)?;
            with_vk(line, |_| Ok(Value::None))?;
            VULKAN.with(|cell| *cell.borrow_mut() = None);
            Ok(Value::None)
        }
        "poll" => {
            expect_arity("vk.poll", args, 0, 0, line)?;
            with_vk(line, |state| Ok(Value::Bool(state.poll())))
        }
        "shouldClose" | "should_close" => {
            expect_arity("vk.shouldClose", args, 0, 0, line)?;
            with_vk(line, |state| Ok(Value::Bool(!state.running)))
        }
        "present" | "swap" | "clear" => {
            expect_arity("vk.present", args, 0, 0, line)?;
            with_vk(line, |state| state.present(line))
        }
        "clearColor" | "clear_color" => {
            expect_arity("vk.clearColor", args, 3, 4, line)?;
            let red = float_arg("vk.clearColor", &args[0], line)? as f32;
            let green = float_arg("vk.clearColor", &args[1], line)? as f32;
            let blue = float_arg("vk.clearColor", &args[2], line)? as f32;
            let alpha = if args.len() == 4 {
                float_arg("vk.clearColor", &args[3], line)? as f32
            } else {
                1.0
            };
            with_vk(line, |state| {
                state.clear_color = [red, green, blue, alpha];
                Ok(Value::None)
            })
        }
        "setTitle" | "set_title" => {
            expect_arity("vk.setTitle", args, 1, 1, line)?;
            let title = text_arg("vk.setTitle", &args[0], line)?;
            with_vk(line, |state| {
                state
                    .window
                    .set_title(&title)
                    .map_err(|message| Error::new(line, format!("vk.setTitle failed: {message}")))?;
                Ok(Value::None)
            })
        }
        "size" => {
            expect_arity("vk.size", args, 0, 0, line)?;
            with_vk(line, |state| {
                let (width, height) = state.window.size();
                Ok(Value::list(vec![
                    Value::Int(width as i64),
                    Value::Int(height as i64),
                ]))
            })
        }
        "deviceName" | "device_name" => {
            expect_arity("vk.deviceName", args, 0, 0, line)?;
            with_vk(line, |state| Ok(Value::Text(state.device_name.clone())))
        }
        "apiVersion" | "api_version" => {
            expect_arity("vk.apiVersion", args, 0, 0, line)?;
            with_vk(line, |state| {
                Ok(Value::Text(vulkan_version(state.api_version)))
            })
        }
        "waitIdle" | "wait_idle" => {
            expect_arity("vk.waitIdle", args, 0, 0, line)?;
            with_vk(line, |state| {
                unsafe {
                    state
                        .device
                        .device_wait_idle()
                        .map_err(|error| vk_error("vk.waitIdle", error, line))?;
                }
                Ok(Value::None)
            })
        }
        other => Err(unknown_method("vk", other, line)),
    }
}

fn vk_init(args: &[Value], line: usize) -> Result<Value, Error> {
    expect_arity("vk.init", args, 3, 3, line)?;
    let title = text_arg("vk.init", &args[0], line)?;
    let width = dimension_arg("vk.init", &args[1], line)?;
    let height = dimension_arg("vk.init", &args[2], line)?;

    VULKAN.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.is_some() {
            return Err(Error::new(
                line,
                "a Vulkan window is already open; call vk.close() first",
            ));
        }
        *state = Some(VkState::create(&title, width, height, line)?);
        Ok(Value::None)
    })
}

fn with_vk<T>(
    line: usize,
    function: impl FnOnce(&mut VkState) -> Result<T, Error>,
) -> Result<T, Error> {
    VULKAN.with(|cell| {
        let mut state = cell.borrow_mut();
        let state = state.as_mut().ok_or_else(|| {
            Error::new(
                line,
                "Vulkan is not initialized; call vk.init(title, width, height)",
            )
        })?;
        function(state)
    })
}

const VK_MAX_FRAMES: usize = 2;

struct VkState {
    _sdl: Sdl,
    _video: VideoSubsystem,
    window: Window,
    events: EventPump,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    surface: ash::vk::SurfaceKHR,
    _physical_device: ash::vk::PhysicalDevice,
    device_name: String,
    api_version: u32,
    device: ash::Device,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: ash::vk::SwapchainKHR,
    _swapchain_format: ash::vk::Format,
    swapchain_extent: ash::vk::Extent2D,
    _images: Vec<ash::vk::Image>,
    image_views: Vec<ash::vk::ImageView>,
    render_pass: ash::vk::RenderPass,
    framebuffers: Vec<ash::vk::Framebuffer>,
    command_pool: ash::vk::CommandPool,
    command_buffers: Vec<ash::vk::CommandBuffer>,
    image_available: Vec<ash::vk::Semaphore>,
    render_finished: Vec<ash::vk::Semaphore>,
    in_flight: Vec<ash::vk::Fence>,
    queue: ash::vk::Queue,
    current_frame: usize,
    clear_color: [f32; 4],
    running: bool,
}

impl VkState {
    #[allow(clippy::too_many_lines)]
    fn create(title: &str, width: u32, height: u32, line: usize) -> Result<Self, Error> {
        use std::ffi::{CString, c_char};

        let sdl = sdl2::init()
            .map_err(|message| Error::new(line, format!("could not initialize SDL: {message}")))?;
        let video = sdl.video().map_err(|message| {
            Error::new(line, format!("could not initialize SDL video: {message}"))
        })?;
        video.vulkan_load_library_default().map_err(|message| {
            Error::new(line, format!("could not load the Vulkan library: {message}"))
        })?;

        let window = video
            .window(title, width, height)
            .vulkan()
            .resizable()
            .build()
            .map_err(|message| {
                Error::new(line, format!("could not create Vulkan window: {message}"))
            })?;

        let extension_names = window
            .vulkan_instance_extensions()
            .map_err(|message| {
                Error::new(
                    line,
                    format!("could not query Vulkan instance extensions: {message}"),
                )
            })?;
        let extension_strings: Vec<CString> = extension_names
            .iter()
            .map(|name| CString::new(*name).expect("SDL Vulkan extension names contain no NUL"))
            .collect();
        let extension_pointers: Vec<*const c_char> =
            extension_strings.iter().map(|name| name.as_ptr()).collect();

        let entry = ash::Entry::linked();
        let application_name = CString::new("DSharp2").expect("static text contains no NUL");
        let engine_name = CString::new("DSharp2").expect("static text contains no NUL");
        let application_info = ash::vk::ApplicationInfo::default()
            .application_name(&application_name)
            .application_version(ash::vk::make_api_version(0, 1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(ash::vk::make_api_version(0, 1, 0, 0))
            .api_version(ash::vk::make_api_version(0, 1, 3, 0));
        let instance_info = ash::vk::InstanceCreateInfo::default()
            .application_info(&application_info)
            .enabled_extension_names(&extension_pointers);
        let instance = unsafe { entry.create_instance(&instance_info, None) }
            .map_err(|error| vk_error("could not create a Vulkan instance", error, line))?;

        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let surface_raw = window.vulkan_create_surface(instance.handle().as_raw() as usize);
        let surface = match surface_raw {
            Ok(surface) => ash::vk::SurfaceKHR::from_raw(surface),
            Err(message) => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(Error::new(
                    line,
                    format!("could not create a Vulkan surface: {message}"),
                ));
            }
        };

        let devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|error| vk_error("could not list Vulkan devices", error, line))?;

        let mut chosen = None;
        'devices: for physical_device in devices {
            let families =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
            for (index, family) in families.iter().enumerate() {
                if !family
                    .queue_flags
                    .contains(ash::vk::QueueFlags::GRAPHICS)
                {
                    continue;
                }
                let present_supported = unsafe {
                    surface_loader.get_physical_device_surface_support(
                        physical_device,
                        index as u32,
                        surface,
                    )
                }
                .unwrap_or(false);
                if present_supported {
                    chosen = Some((physical_device, index as u32));
                    break 'devices;
                }
            }
        }

        let Some((physical_device, queue_family)) = chosen else {
            unsafe {
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
            }
            return Err(Error::new(
                line,
                "no Vulkan device with a graphics queue that can present to the window was found",
            ));
        };

        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_name = properties.device_name_as_c_str()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "unknown Vulkan device".to_string());
        let api_version = properties.api_version;

        let priorities = [1.0f32];
        let queue_info = ash::vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities);
        let device_extensions = [ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()];
        let device_info = ash::vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extensions);
        let device = unsafe { instance.create_device(physical_device, &device_info, None) }
            .map_err(|error| vk_error("could not create a Vulkan device", error, line))?;
        let queue = unsafe { device.get_device_queue(queue_family, 0) };

        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        let swapchain_state = unsafe {
            create_swapchain(
                &surface_loader,
                &swapchain_loader,
                &device,
                physical_device,
                surface,
                queue_family,
                &window,
            )
        }
        .map_err(|error| vk_error("could not create a Vulkan swapchain", error, line))?;
        let SwapchainState {
            swapchain,
            format,
            extent,
            images,
            image_views,
        } = swapchain_state;

        let render_pass = unsafe { create_render_pass(&device, format) }
            .map_err(|error| vk_error("could not create a Vulkan render pass", error, line))?;
        let framebuffers =
            unsafe { create_framebuffers(&device, render_pass, &image_views, extent) }
                .map_err(|error| {
                    vk_error("could not create Vulkan framebuffers", error, line)
                })?;

        let pool_info = ash::vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(|error| vk_error("could not create a command pool", error, line))?;
        let allocate_info = ash::vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(ash::vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(VK_MAX_FRAMES as u32);
        let command_buffers = unsafe { device.allocate_command_buffers(&allocate_info) }
            .map_err(|error| vk_error("could not allocate command buffers", error, line))?;

        let semaphore_info = ash::vk::SemaphoreCreateInfo::default();
        let fence_info = ash::vk::FenceCreateInfo::default()
            .flags(ash::vk::FenceCreateFlags::SIGNALED);
        let mut image_available = Vec::with_capacity(VK_MAX_FRAMES);
        let mut render_finished = Vec::with_capacity(VK_MAX_FRAMES);
        let mut in_flight = Vec::with_capacity(VK_MAX_FRAMES);
        for _ in 0..VK_MAX_FRAMES {
            image_available.push(unsafe { device.create_semaphore(&semaphore_info, None) }
                .map_err(|error| vk_error("could not create a semaphore", error, line))?);
            render_finished.push(unsafe { device.create_semaphore(&semaphore_info, None) }
                .map_err(|error| vk_error("could not create a semaphore", error, line))?);
            in_flight.push(unsafe { device.create_fence(&fence_info, None) }
                .map_err(|error| vk_error("could not create a fence", error, line))?);
        }

        let events = sdl
            .event_pump()
            .map_err(|message| Error::new(line, format!("could not read SDL events: {message}")))?;

        Ok(VkState {
            _sdl: sdl,
            _video: video,
            window,
            events,
            _entry: entry,
            instance,
            surface_loader,
            surface,
            _physical_device: physical_device,
            device_name,
            api_version,
            device,
            swapchain_loader,
            swapchain,
            _swapchain_format: format.format,
            swapchain_extent: extent,
            _images: images,
            image_views,
            render_pass,
            framebuffers,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
            queue,
            current_frame: 0,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            running: true,
        })
    }

    fn poll(&mut self) -> bool {
        for event in self.events.poll_iter() {
            match event {
                Event::Quit { .. } => self.running = false,
                Event::Window {
                    win_event: WindowEvent::Close,
                    ..
                } => self.running = false,
                _ => {}
            }
        }

        self.running
    }

    fn present(&mut self, line: usize) -> Result<Value, Error> {
        unsafe {
            let fence = self.in_flight[self.current_frame];
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .map_err(|error| vk_error("vk.present", error, line))?;

            let (image_index, _) = match self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available[self.current_frame],
                ash::vk::Fence::null(),
            ) {
                Ok(result) => result,
                Err(ash::vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    return Err(Error::new(
                        line,
                        "Vulkan swapchain is out of date; close and re-open the window",
                    ));
                }
                Err(error) => return Err(vk_error("vk.present", error, line)),
            };

            self.device
                .reset_fences(&[fence])
                .map_err(|error| vk_error("vk.present", error, line))?;

            let command_buffer = self.command_buffers[self.current_frame];
            self.device
                .reset_command_buffer(command_buffer, ash::vk::CommandBufferResetFlags::empty())
                .map_err(|error| vk_error("vk.present", error, line))?;

            let begin_info = ash::vk::CommandBufferBeginInfo::default();
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|error| vk_error("vk.present", error, line))?;

            let clear_value = ash::vk::ClearValue {
                color: ash::vk::ClearColorValue {
                    float32: self.clear_color,
                },
            };
            let render_area = ash::vk::Rect2D::default()
                .offset(ash::vk::Offset2D::default().x(0).y(0))
                .extent(self.swapchain_extent);
            let render_pass_info = ash::vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index as usize])
                .render_area(render_area)
                .clear_values(std::slice::from_ref(&clear_value));
            self.device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                ash::vk::SubpassContents::INLINE,
            );
            self.device.cmd_end_render_pass(command_buffer);
            self.device
                .end_command_buffer(command_buffer)
                .map_err(|error| vk_error("vk.present", error, line))?;

            let wait_semaphores = [self.image_available[self.current_frame]];
            let wait_stages = [ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_semaphores = [self.render_finished[self.current_frame]];
            let command_buffers = [command_buffer];
            let submit_info = ash::vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            self.device
                .queue_submit(self.queue, &[submit_info], fence)
                .map_err(|error| vk_error("vk.present", error, line))?;

            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            let present_info = ash::vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            match self
                .swapchain_loader
                .queue_present(self.queue, &present_info)
            {
                Ok(_) => {}
                Err(ash::vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    return Err(Error::new(
                        line,
                        "Vulkan swapchain is out of date; close and re-open the window",
                    ));
                }
                Err(error) => return Err(vk_error("vk.present", error, line)),
            }

            self.current_frame = (self.current_frame + 1) % VK_MAX_FRAMES;
        }

        Ok(Value::None)
    }
}

impl Drop for VkState {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for semaphore in &self.image_available {
                self.device.destroy_semaphore(*semaphore, None);
            }
            for semaphore in &self.render_finished {
                self.device.destroy_semaphore(*semaphore, None);
            }
            for fence in &self.in_flight {
                self.device.destroy_fence(*fence, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            for framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(*framebuffer, None);
            }
            self.device.destroy_render_pass(self.render_pass, None);
            for image_view in &self.image_views {
                self.device.destroy_image_view(*image_view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

struct SwapchainState {
    swapchain: ash::vk::SwapchainKHR,
    format: ash::vk::SurfaceFormatKHR,
    extent: ash::vk::Extent2D,
    images: Vec<ash::vk::Image>,
    image_views: Vec<ash::vk::ImageView>,
}

unsafe fn create_swapchain(
    surface_loader: &ash::khr::surface::Instance,
    swapchain_loader: &ash::khr::swapchain::Device,
    device: &ash::Device,
    physical_device: ash::vk::PhysicalDevice,
    surface: ash::vk::SurfaceKHR,
    queue_family: u32,
    window: &Window,
) -> Result<SwapchainState, ash::vk::Result> {
    let capabilities =
        unsafe { surface_loader.get_physical_device_surface_capabilities(physical_device, surface)? };
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };
    let present_modes =
        unsafe { surface_loader.get_physical_device_surface_present_modes(physical_device, surface)? };

    let format = formats
        .iter()
        .copied()
        .find(|format| {
            format.format == ash::vk::Format::B8G8R8A8_SRGB
                && format.color_space == ash::vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or_else(|| formats.first().copied())
        .ok_or(ash::vk::Result::ERROR_INITIALIZATION_FAILED)?;

    let present_mode = if present_modes.contains(&ash::vk::PresentModeKHR::MAILBOX) {
        ash::vk::PresentModeKHR::MAILBOX
    } else {
        ash::vk::PresentModeKHR::FIFO
    };

    let extent = if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        let (width, height) = window.size();
        ash::vk::Extent2D {
            width: width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    };

    let mut image_count = capabilities.min_image_count + 1;
    if capabilities.max_image_count > 0 {
        image_count = image_count.min(capabilities.max_image_count);
    }

    let composite_alpha =
        if capabilities
            .supported_composite_alpha
            .contains(ash::vk::CompositeAlphaFlagsKHR::OPAQUE)
        {
            ash::vk::CompositeAlphaFlagsKHR::OPAQUE
        } else {
            ash::vk::CompositeAlphaFlagsKHR::INHERIT
        };

    let create_info = ash::vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(ash::vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(composite_alpha)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(ash::vk::SwapchainKHR::null());
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None)? };
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };

    let mut image_views = Vec::with_capacity(images.len());
    for image in &images {
        let range = ash::vk::ImageSubresourceRange::default()
            .aspect_mask(ash::vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);
        let view_info = ash::vk::ImageViewCreateInfo::default()
            .image(*image)
            .view_type(ash::vk::ImageViewType::TYPE_2D)
            .format(format.format)
            .components(ash::vk::ComponentMapping::default())
            .subresource_range(range);
        image_views.push(unsafe { device.create_image_view(&view_info, None)? });
    }

    let _ = queue_family;
    Ok(SwapchainState {
        swapchain,
        format,
        extent,
        images,
        image_views,
    })
}

unsafe fn create_render_pass(
    device: &ash::Device,
    format: ash::vk::SurfaceFormatKHR,
) -> Result<ash::vk::RenderPass, ash::vk::Result> {
    let attachment = ash::vk::AttachmentDescription::default()
        .format(format.format)
        .samples(ash::vk::SampleCountFlags::TYPE_1)
        .load_op(ash::vk::AttachmentLoadOp::CLEAR)
        .store_op(ash::vk::AttachmentStoreOp::STORE)
        .stencil_load_op(ash::vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(ash::vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(ash::vk::ImageLayout::UNDEFINED)
        .final_layout(ash::vk::ImageLayout::PRESENT_SRC_KHR);
    let attachments = [attachment];

    let color_reference = ash::vk::AttachmentReference::default()
        .attachment(0)
        .layout(ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let color_references = [color_reference];
    let subpass = ash::vk::SubpassDescription::default()
        .pipeline_bind_point(ash::vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_references);
    let dependencies = [ash::vk::SubpassDependency::default()
        .src_subpass(ash::vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(ash::vk::AccessFlags::empty())
        .dst_stage_mask(ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(ash::vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];

    let create_info = ash::vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(&dependencies);
    unsafe { device.create_render_pass(&create_info, None) }
}

unsafe fn create_framebuffers(
    device: &ash::Device,
    render_pass: ash::vk::RenderPass,
    image_views: &[ash::vk::ImageView],
    extent: ash::vk::Extent2D,
) -> Result<Vec<ash::vk::Framebuffer>, ash::vk::Result> {
    let mut framebuffers = Vec::with_capacity(image_views.len());
    for image_view in image_views {
        let attachments = [*image_view];
        let create_info = ash::vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        framebuffers.push(unsafe { device.create_framebuffer(&create_info, None)? });
    }
    Ok(framebuffers)
}

fn vulkan_version(version: u32) -> String {
    format!(
        "{}.{}.{}",
        ash::vk::api_version_major(version),
        ash::vk::api_version_minor(version),
        ash::vk::api_version_patch(version)
    )
}

fn vk_error(context: &str, error: ash::vk::Result, line: usize) -> Error {
    Error::new(line, format!("{context}: {error}"))
}


fn gl_init(args: &[Value], line: usize) -> Result<Value, Error> {
    expect_arity("gl.init", args, 3, 3, line)?;
    let title = text_arg("gl.init", &args[0], line)?;
    let width = dimension_arg("gl.init", &args[1], line)?;
    let height = dimension_arg("gl.init", &args[2], line)?;

    OPENGL.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.is_some() {
            return Err(Error::new(
                line,
                "an OpenGL window is already open; call gl.close() first",
            ));
        }
        *state = Some(GlState::create(&title, width, height, line)?);
        Ok(Value::None)
    })
}

fn with_gl<T>(
    line: usize,
    function: impl FnOnce(&mut GlState) -> Result<T, Error>,
) -> Result<T, Error> {
    OPENGL.with(|cell| {
        let mut state = cell.borrow_mut();
        let state = state.as_mut().ok_or_else(|| {
            Error::new(line, "OpenGL is not initialized; call gl.init(title, width, height)")
        })?;
        function(state)
    })
}

struct GlState {
    gl: glow::Context,
    _context: GLContext,
    window: Window,
    events: EventPump,
    _video: VideoSubsystem,
    _sdl: Sdl,
    running: bool,
    next_handle: u32,
    shaders: HashMap<u32, glow::Shader>,
    programs: HashMap<u32, glow::Program>,
    buffers: HashMap<u32, glow::Buffer>,
    vertex_arrays: HashMap<u32, glow::VertexArray>,
    uniforms: HashMap<u32, glow::UniformLocation>,
}

impl GlState {
    fn create(title: &str, width: u32, height: u32, line: usize) -> Result<Self, Error> {
        let sdl = sdl2::init()
            .map_err(|message| Error::new(line, format!("could not initialize SDL: {message}")))?;
        let video = sdl.video().map_err(|message| {
            Error::new(line, format!("could not initialize SDL video: {message}"))
        })?;

        let attributes = video.gl_attr();
        attributes.set_context_profile(GLProfile::Core);
        attributes.set_context_version(3, 3);
        attributes.set_double_buffer(true);
        attributes.set_depth_size(24);

        let window = video
            .window(title, width, height)
            .opengl()
            .resizable()
            .build()
            .map_err(|message| {
                Error::new(line, format!("could not create OpenGL window: {message}"))
            })?;
        let context = window.gl_create_context().map_err(|message| {
            Error::new(line, format!("could not create OpenGL context: {message}"))
        })?;
        window.gl_make_current(&context).map_err(|message| {
            Error::new(line, format!("could not activate OpenGL context: {message}"))
        })?;

        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                video.gl_get_proc_address(name) as *const c_void
            })
        };
        let events = sdl
            .event_pump()
            .map_err(|message| Error::new(line, format!("could not read SDL events: {message}")))?;

        Ok(GlState {
            gl,
            _context: context,
            window,
            events,
            _video: video,
            _sdl: sdl,
            running: true,
            next_handle: 1,
            shaders: HashMap::new(),
            programs: HashMap::new(),
            buffers: HashMap::new(),
            vertex_arrays: HashMap::new(),
            uniforms: HashMap::new(),
        })
    }

    fn poll(&mut self) -> bool {
        for event in self.events.poll_iter() {
            match event {
                Event::Quit { .. } => self.running = false,
                Event::Window {
                    win_event: WindowEvent::Close,
                    ..
                } => self.running = false,
                Event::Window {
                    win_event: WindowEvent::SizeChanged(width, height)
                    | WindowEvent::Resized(width, height),
                    ..
                } if width > 0 && height > 0 => {
                    unsafe {
                        self.gl.viewport(0, 0, width, height);
                    }
                }
                _ => {}
            }
        }

        self.running
    }

    fn handle(&mut self) -> u32 {
        let handle = self.next_handle;
        self.next_handle += 1;
        handle
    }

    fn create_shader(&mut self, stage: &str, source: &str, line: usize) -> Result<Value, Error> {
        let shader_type = match stage.to_ascii_lowercase().as_str() {
            "vertex" | "vs" => glow::VERTEX_SHADER,
            "fragment" | "fs" | "pixel" => glow::FRAGMENT_SHADER,
            other => {
                return Err(Error::new(
                    line,
                    format!("unknown shader stage '{other}' (use 'vertex' or 'fragment')"),
                ));
            }
        };

        let shader = unsafe { self.gl.create_shader(shader_type) }
            .map_err(|message| Error::new(line, format!("could not create shader: {message}")))?;
        unsafe {
            self.gl.shader_source(shader, source);
            self.gl.compile_shader(shader);
        }

        if !unsafe { self.gl.get_shader_compile_status(shader) } {
            let log = unsafe { self.gl.get_shader_info_log(shader) };
            unsafe {
                self.gl.delete_shader(shader);
            }
            return Err(Error::new(
                line,
                format!("shader compilation failed: {log}"),
            ));
        }

        let handle = self.handle();
        self.shaders.insert(handle, shader);
        Ok(Value::Int(handle as i64))
    }

    fn delete_shader(&mut self, handle: u32, line: usize) -> Result<Value, Error> {
        let shader = self
            .shaders
            .remove(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown shader handle {handle}")))?;
        unsafe {
            self.gl.delete_shader(shader);
        }
        Ok(Value::None)
    }

    fn create_program(
        &mut self,
        vertex: u32,
        fragment: u32,
        line: usize,
    ) -> Result<Value, Error> {
        let vertex_shader = *self
            .shaders
            .get(&vertex)
            .ok_or_else(|| Error::new(line, format!("unknown vertex shader handle {vertex}")))?;
        let fragment_shader = *self
            .shaders
            .get(&fragment)
            .ok_or_else(|| Error::new(line, format!("unknown fragment shader handle {fragment}")))?;

        let program = unsafe { self.gl.create_program() }
            .map_err(|message| Error::new(line, format!("could not create program: {message}")))?;
        unsafe {
            self.gl.attach_shader(program, vertex_shader);
            self.gl.attach_shader(program, fragment_shader);
            self.gl.link_program(program);
        }

        if !unsafe { self.gl.get_program_link_status(program) } {
            let log = unsafe { self.gl.get_program_info_log(program) };
            unsafe {
                self.gl.detach_shader(program, vertex_shader);
                self.gl.detach_shader(program, fragment_shader);
                self.gl.delete_program(program);
            }
            return Err(Error::new(line, format!("program linking failed: {log}")));
        }

        unsafe {
            self.gl.detach_shader(program, vertex_shader);
            self.gl.detach_shader(program, fragment_shader);
        }

        let handle = self.handle();
        self.programs.insert(handle, program);
        Ok(Value::Int(handle as i64))
    }

    fn use_program(&mut self, handle: u32, line: usize) -> Result<Value, Error> {
        let program = *self
            .programs
            .get(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown program handle {handle}")))?;
        unsafe {
            self.gl.use_program(Some(program));
        }
        Ok(Value::None)
    }

    fn delete_program(&mut self, handle: u32, line: usize) -> Result<Value, Error> {
        let program = self
            .programs
            .remove(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown program handle {handle}")))?;
        unsafe {
            self.gl.delete_program(program);
        }
        Ok(Value::None)
    }

    fn create_buffer(&mut self, line: usize) -> Result<Value, Error> {
        let buffer = unsafe { self.gl.create_buffer() }
            .map_err(|message| Error::new(line, format!("could not create buffer: {message}")))?;
        let handle = self.handle();
        self.buffers.insert(handle, buffer);
        Ok(Value::Int(handle as i64))
    }

    fn delete_buffer(&mut self, handle: u32, line: usize) -> Result<Value, Error> {
        let buffer = self
            .buffers
            .remove(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown buffer handle {handle}")))?;
        unsafe {
            self.gl.delete_buffer(buffer);
        }
        Ok(Value::None)
    }

    fn bind_buffer(
        &mut self,
        target: u32,
        handle: u32,
        line: usize,
    ) -> Result<Value, Error> {
        let buffer = *self
            .buffers
            .get(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown buffer handle {handle}")))?;
        unsafe {
            self.gl.bind_buffer(target, Some(buffer));
        }
        Ok(Value::None)
    }

    fn create_vertex_array(&mut self, line: usize) -> Result<Value, Error> {
        let vertex_array = unsafe { self.gl.create_vertex_array() }.map_err(|message| {
            Error::new(line, format!("could not create vertex array: {message}"))
        })?;
        let handle = self.handle();
        self.vertex_arrays.insert(handle, vertex_array);
        Ok(Value::Int(handle as i64))
    }

    fn delete_vertex_array(&mut self, handle: u32, line: usize) -> Result<Value, Error> {
        let vertex_array = self
            .vertex_arrays
            .remove(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown vertex array handle {handle}")))?;
        unsafe {
            self.gl.delete_vertex_array(vertex_array);
        }
        Ok(Value::None)
    }

    fn bind_vertex_array(&mut self, handle: u32, line: usize) -> Result<Value, Error> {
        let vertex_array = *self
            .vertex_arrays
            .get(&handle)
            .ok_or_else(|| Error::new(line, format!("unknown vertex array handle {handle}")))?;
        unsafe {
            self.gl.bind_vertex_array(Some(vertex_array));
        }
        Ok(Value::None)
    }

    fn get_uniform_location(
        &mut self,
        program: u32,
        name: &str,
        line: usize,
    ) -> Result<Value, Error> {
        let program = *self
            .programs
            .get(&program)
            .ok_or_else(|| Error::new(line, format!("unknown program handle {program}")))?;
        let location = unsafe { self.gl.get_uniform_location(program, name) }.ok_or_else(|| {
            Error::new(
                line,
                format!("uniform '{name}' was not found (or was optimized out)"),
            )
        })?;
        let handle = self.handle();
        self.uniforms.insert(handle, location);
        Ok(Value::Int(handle as i64))
    }

    fn uniform(
        &self,
        handle: u32,
        line: usize,
    ) -> Result<glow::UniformLocation, Error> {
        self.uniforms
            .get(&handle)
            .copied()
            .ok_or_else(|| Error::new(line, format!("unknown uniform handle {handle}")))
    }

    fn uniform_f32(
        &mut self,
        handle: u32,
        values: &[f32],
        line: usize,
    ) -> Result<Value, Error> {
        let location = self.uniform(handle, line)?;
        unsafe {
            match values.len() {
                1 => self.gl.uniform_1_f32(Some(&location), values[0]),
                2 => self
                    .gl
                    .uniform_2_f32(Some(&location), values[0], values[1]),
                3 => self
                    .gl
                    .uniform_3_f32(Some(&location), values[0], values[1], values[2]),
                4 => self.gl.uniform_4_f32(
                    Some(&location),
                    values[0],
                    values[1],
                    values[2],
                    values[3],
                ),
                _ => unreachable!("uniform_f32 is only called with 1 to 4 values"),
            }
        }
        Ok(Value::None)
    }

    fn uniform_i32(&mut self, handle: u32, value: i32, line: usize) -> Result<Value, Error> {
        let location = self.uniform(handle, line)?;
        unsafe {
            self.gl.uniform_1_i32(Some(&location), value);
        }
        Ok(Value::None)
    }
}

fn expect_arity(
    name: &str,
    args: &[Value],
    minimum: usize,
    maximum: usize,
    line: usize,
) -> Result<(), Error> {
    if args.len() < minimum || args.len() > maximum {
        let expected = if minimum == maximum {
            format!("{minimum}")
        } else {
            format!("{minimum} to {maximum}")
        };
        return Err(Error::new(
            line,
            format!(
                "{name}() takes {expected} argument(s), but {} given",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn int_arg(name: &str, value: &Value, line: usize) -> Result<i64, Error> {
    match value {
        Value::Int(number) => Ok(*number),
        other => Err(Error::new(
            line,
            format!("{name}() expected Int, got {}", other.type_name()),
        )),
    }
}

fn index_arg(name: &str, value: &Value, line: usize) -> Result<u32, Error> {
    let number = int_arg(name, value, line)?;
    u32::try_from(number).map_err(|_| {
        Error::new(line, format!("{name}() index must be between 0 and {}", u32::MAX))
    })
}

fn dimension_arg(name: &str, value: &Value, line: usize) -> Result<u32, Error> {
    let number = int_arg(name, value, line)?;
    u32::try_from(number).map_err(|_| {
        Error::new(
            line,
            format!("{name}() size must be a positive 32-bit integer"),
        )
    })
}

fn handle_arg(name: &str, value: &Value, line: usize) -> Result<u32, Error> {
    index_arg(name, value, line)
}

fn float_arg(name: &str, value: &Value, line: usize) -> Result<f64, Error> {
    match value {
        Value::Int(number) => Ok(*number as f64),
        Value::Float(number) => Ok(*number),
        other => Err(Error::new(
            line,
            format!("{name}() expected a number, got {}", other.type_name()),
        )),
    }
}

fn bool_arg(name: &str, value: &Value, line: usize) -> Result<bool, Error> {
    match value {
        Value::Bool(value) => Ok(*value),
        other => Err(Error::new(
            line,
            format!("{name}() expected Bool, got {}", other.type_name()),
        )),
    }
}

fn text_arg(name: &str, value: &Value, line: usize) -> Result<String, Error> {
    match value {
        Value::Text(text) => Ok(text.clone()),
        other => Err(Error::new(
            line,
            format!("{name}() expected Text, got {}", other.type_name()),
        )),
    }
}

fn matrix_values(value: &Value, line: usize) -> Result<[f32; 16], Error> {
    let Value::List(list) = value else {
        return Err(Error::new(
            line,
            format!(
                "gl.uniformMatrix4fv() expected a list of 16 numbers, got {}",
                value.type_name()
            ),
        ));
    };

    let items = list.items.borrow();
    if items.len() != 16 {
        return Err(Error::new(
            line,
            format!(
                "gl.uniformMatrix4fv() expected 16 numbers, got {}",
                items.len()
            ),
        ));
    }

    let mut values = [0.0f32; 16];
    for (index, item) in items.iter().enumerate() {
        values[index] = float_arg("gl.uniformMatrix4fv", item, line)? as f32;
    }
    Ok(values)
}

fn buffer_bytes(
    value: &Value,
    data_type: Option<&str>,
    line: usize,
) -> Result<Vec<u8>, Error> {
    let Value::List(list) = value else {
        return Err(Error::new(
            line,
            format!(
                "gl.bufferData() expected a list of numbers, got {}",
                value.type_name()
            ),
        ));
    };

    let data_type = data_type.unwrap_or("f32").to_ascii_lowercase();
    let items = list.items.borrow();
    let mut bytes = Vec::new();

    match data_type.as_str() {
        "f32" | "float" => {
            bytes.reserve(items.len() * 4);
            for item in items.iter() {
                bytes.extend_from_slice(&(float_arg("gl.bufferData", item, line)? as f32).to_le_bytes());
            }
        }
        "u32" | "uint" => {
            bytes.reserve(items.len() * 4);
            for item in items.iter() {
                let number = u32::try_from(int_arg("gl.bufferData", item, line)?)
                    .map_err(|_| Error::new(line, "gl.bufferData() uint value is out of range"))?;
                bytes.extend_from_slice(&number.to_le_bytes());
            }
        }
        "i32" | "int" => {
            bytes.reserve(items.len() * 4);
            for item in items.iter() {
                let number = i32::try_from(int_arg("gl.bufferData", item, line)?)
                    .map_err(|_| Error::new(line, "gl.bufferData() int value is out of range"))?;
                bytes.extend_from_slice(&number.to_le_bytes());
            }
        }
        "u16" | "ushort" => {
            bytes.reserve(items.len() * 2);
            for item in items.iter() {
                let number = u16::try_from(int_arg("gl.bufferData", item, line)?)
                    .map_err(|_| Error::new(line, "gl.bufferData() ushort value is out of range"))?;
                bytes.extend_from_slice(&number.to_le_bytes());
            }
        }
        "u8" | "ubyte" => {
            bytes.reserve(items.len());
            for item in items.iter() {
                let number = u8::try_from(int_arg("gl.bufferData", item, line)?)
                    .map_err(|_| Error::new(line, "gl.bufferData() ubyte value is out of range"))?;
                bytes.push(number);
            }
        }
        other => {
            return Err(Error::new(
                line,
                format!(
                    "unknown buffer data type '{other}' (use f32, u32, i32, u16, or u8)"
                ),
            ));
        }
    }

    Ok(bytes)
}

fn clear_mask(text: &str, line: usize) -> Result<u32, Error> {
    let mut mask = 0;
    for part in text
        .to_ascii_lowercase()
        .split(|character: char| character == '|' || character == ',' || character.is_whitespace())
        .filter(|part| !part.is_empty())
    {
        mask |= match part {
            "color" | "colour" => glow::COLOR_BUFFER_BIT,
            "depth" => glow::DEPTH_BUFFER_BIT,
            "stencil" => glow::STENCIL_BUFFER_BIT,
            other => {
                return Err(Error::new(
                    line,
                    format!("unknown clear buffer '{other}' (use color, depth, or stencil)"),
                ));
            }
        };
    }

    if mask == 0 {
        return Err(Error::new(
            line,
            "gl.clear() needs at least one of color, depth, or stencil",
        ));
    }

    Ok(mask)
}

fn string_constant(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "vendor" => glow::VENDOR,
        "renderer" => glow::RENDERER,
        "version" => glow::VERSION,
        "glsl" | "shading_language" | "shading-language" => glow::SHADING_LANGUAGE_VERSION,
        other => {
            return Err(Error::new(
                line,
                format!(
                    "unknown GL string '{other}' (use vendor, renderer, version, or glsl)"
                ),
            ));
        }
    })
}

fn capability_constant(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "blend" | "blending" => glow::BLEND,
        "depth_test" | "depth-test" | "depth" => glow::DEPTH_TEST,
        "cull_face" | "cull-face" | "culling" => glow::CULL_FACE,
        "scissor_test" | "scissor-test" | "scissor" => glow::SCISSOR_TEST,
        "stencil_test" | "stencil-test" | "stencil" => glow::STENCIL_TEST,
        other => {
            return Err(Error::new(
                line,
                format!(
                    "unknown GL capability '{other}' (use blend, depth_test, cull_face, scissor_test, or stencil_test)"
                ),
            ));
        }
    })
}

fn blend_constant(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "zero" => glow::ZERO,
        "one" => glow::ONE,
        "src_alpha" | "src-alpha" => glow::SRC_ALPHA,
        "one_minus_src_alpha" | "one-minus-src-alpha" => glow::ONE_MINUS_SRC_ALPHA,
        "dst_alpha" | "dst-alpha" => glow::DST_ALPHA,
        "one_minus_dst_alpha" | "one-minus-dst-alpha" => glow::ONE_MINUS_DST_ALPHA,
        "src_color" | "src-color" => glow::SRC_COLOR,
        "one_minus_src_color" | "one-minus-src-color" => glow::ONE_MINUS_SRC_COLOR,
        other => {
            return Err(Error::new(
                line,
                format!("unknown blend factor '{other}'"),
            ));
        }
    })
}

fn depth_constant(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "less" => glow::LESS,
        "less_equal" | "less-equal" => glow::LEQUAL,
        "equal" => glow::EQUAL,
        "greater" => glow::GREATER,
        "greater_equal" | "greater-equal" => glow::GEQUAL,
        "always" => glow::ALWAYS,
        "never" => glow::NEVER,
        other => {
            return Err(Error::new(
                line,
                format!("unknown depth function '{other}'"),
            ));
        }
    })
}

fn face_constant(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "front" => glow::FRONT,
        "back" => glow::BACK,
        "front_and_back" | "front-and-back" | "both" => glow::FRONT_AND_BACK,
        other => {
            return Err(Error::new(line, format!("unknown face '{other}'")));
        }
    })
}

fn buffer_target(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "array" | "vertex" => glow::ARRAY_BUFFER,
        "element" | "index" => glow::ELEMENT_ARRAY_BUFFER,
        "uniform" => glow::UNIFORM_BUFFER,
        other => {
            return Err(Error::new(
                line,
                format!(
                    "unknown buffer target '{other}' (use array, element, or uniform)"
                ),
            ));
        }
    })
}

fn buffer_usage(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "static" => glow::STATIC_DRAW,
        "dynamic" => glow::DYNAMIC_DRAW,
        "stream" => glow::STREAM_DRAW,
        other => {
            return Err(Error::new(
                line,
                format!("unknown buffer usage '{other}' (use static, dynamic, or stream)"),
            ));
        }
    })
}

fn vertex_type(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "float" | "f32" => glow::FLOAT,
        "uint" | "u32" => glow::UNSIGNED_INT,
        "int" | "i32" => glow::INT,
        "ushort" | "u16" => glow::UNSIGNED_SHORT,
        "short" | "i16" => glow::SHORT,
        "ubyte" | "u8" => glow::UNSIGNED_BYTE,
        "byte" | "i8" => glow::BYTE,
        other => {
            return Err(Error::new(
                line,
                format!("unknown vertex attribute type '{other}'"),
            ));
        }
    })
}

fn draw_mode(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "points" => glow::POINTS,
        "lines" => glow::LINES,
        "line_strip" | "line-strip" => glow::LINE_STRIP,
        "line_loop" | "line-loop" => glow::LINE_LOOP,
        "triangles" => glow::TRIANGLES,
        "triangle_strip" | "triangle-strip" => glow::TRIANGLE_STRIP,
        "triangle_fan" | "triangle-fan" => glow::TRIANGLE_FAN,
        other => {
            return Err(Error::new(
                line,
                format!("unknown draw mode '{other}'"),
            ));
        }
    })
}

fn index_type(name: &str, line: usize) -> Result<u32, Error> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "u8" | "ubyte" | "unsigned_byte" => glow::UNSIGNED_BYTE,
        "u16" | "ushort" | "unsigned_short" => glow::UNSIGNED_SHORT,
        "u32" | "uint" | "unsigned_int" => glow::UNSIGNED_INT,
        other => {
            return Err(Error::new(
                line,
                format!("unknown index type '{other}' (use u8, u16, or u32)"),
            ));
        }
    })
}

fn gl_error_name(code: u32) -> &'static str {
    match code {
        glow::NO_ERROR => "NO_ERROR",
        glow::INVALID_ENUM => "INVALID_ENUM",
        glow::INVALID_VALUE => "INVALID_VALUE",
        glow::INVALID_OPERATION => "INVALID_OPERATION",
        glow::INVALID_FRAMEBUFFER_OPERATION => "INVALID_FRAMEBUFFER_OPERATION",
        glow::OUT_OF_MEMORY => "OUT_OF_MEMORY",
        _ => "UNKNOWN_ERROR",
    }
}

fn unknown_method(namespace: &str, method: &str, line: usize) -> Error {
    Error::new(
        line,
        format!("unknown {namespace} method '{namespace}.{method}'"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_mask_accepts_names_and_combinations() {
        assert_eq!(clear_mask("color", 1).unwrap(), glow::COLOR_BUFFER_BIT);
        assert_eq!(
            clear_mask("color | depth", 1).unwrap(),
            glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT
        );
        assert!(clear_mask("", 1).is_err());
        assert!(clear_mask("colour, stencil", 1).is_ok());
        assert!(clear_mask("everything", 1).is_err());
    }

    #[test]
    fn draw_modes_and_gl_errors_have_names() {
        assert_eq!(draw_mode("triangles", 1).unwrap(), glow::TRIANGLES);
        assert_eq!(draw_mode("line_strip", 1).unwrap(), glow::LINE_STRIP);
        assert_eq!(gl_error_name(glow::NO_ERROR), "NO_ERROR");
        assert_eq!(gl_error_name(glow::INVALID_VALUE), "INVALID_VALUE");
        assert_eq!(gl_error_name(0xffff_ffff), "UNKNOWN_ERROR");
    }

    #[test]
    fn vulkan_versions_are_formatted() {
        assert_eq!(
            vulkan_version(ash::vk::make_api_version(0, 1, 4, 354)),
            "1.4.354"
        );
    }
}
