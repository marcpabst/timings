use std::borrow::Cow;
use wgpu::hal::Adapter;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};

use serialport::SerialPort;

use windows::Win32::Graphics::Dxgi::DXGI_FRAME_STATISTICS;

async fn run(event_loop: EventLoop<()>, window: Window) {
    let mut size = window.inner_size();
    size.width = size.width.max(1);
    size.height = size.height.max(1);

    let instance_desc = wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..Default::default()
    };
    let instance = wgpu::Instance::new(instance_desc);

    let mut surface = instance.create_surface(&window).unwrap();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            // Request an adapter which can render to our surface
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    // Create the logical device and command queue
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        )
        .await
        .expect("Failed to create device");

    // Load the shaders from disk
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let mut config = surface
        .get_default_config(&adapter, size.width, size.height)
        .unwrap();

    config.present_mode = wgpu::PresentMode::Fifo;
    config.desired_maximum_frame_latency = 1;
    surface.configure(&device, &config);

    unsafe {
        surface.as_hal::<wgpu::core::api::Dx12,_,_>(
            |surface| {

                let sc = surface.unwrap().swap_chain();
                let sc = sc.as_ref().unwrap();
                let sc2 = sc.raw_swap_chain();
                sc2.SetFullscreenState(true, None)
            }
        )
    };

    // open serial port
    let mut port = serialport::new("COM3", 115200)
        .timeout(std::time::Duration::from_millis(100))
        .data_bits(serialport::DataBits::Eight)
        .flow_control(serialport::FlowControl::None)
        .open()
        .expect("Failed to open serial port");

    let mut last_time = 0;
    let mut last_frame = 0;
    let mut last_present_count = 0;
    let mut running_frame: u8 = 0;

    let window = &window;
    event_loop
        .run(move |event, target| {
            // Have the closure take ownership of the resources.
            // `event_loop.run` never returns, therefore we must do this to ensure
            // the resources are properly cleaned up.
            let _ = (&instance, &adapter, &shader, &pipeline_layout);

            if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::Resized(new_size) => {
                        // Reconfigure the surface with the new size
                        config.width = new_size.width.max(1);
                        config.height = new_size.height.max(1);
                        surface.configure(&device, &config);
                        // On macos the window needs to be redrawn manually after resizing
                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {


                        let frame = surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: None,
                            });
                        {
                            let mut rpass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });
                            rpass.set_pipeline(&render_pipeline);
                            if running_frame > 10 {
                              // do nothing
                            } else if running_frame % 2 == 0 {
                                rpass.draw(0..6, 0..1);
                            } else {
                                // do nothing
                            }

                        }

                        queue.submit(Some(encoder.finish()));
                        frame.present();
                        window.request_redraw();

                        let mut present_stats = get_frame_stats(&surface);

                        // busy wait until the flip count changes
                        while present_stats.PresentCount == last_frame {
                            present_stats = get_frame_stats(&surface);
                            // sleep for 1us
                            std::thread::sleep(std::time::Duration::from_micros(1));
                        }


                        // convert to bytes running_frame in
                        if running_frame <= 10 {
                            let payload = (running_frame + 1).to_be_bytes();
                            port.write(&payload).expect("Failed to write to serial port");
                            port.flush().expect("Failed to flush serial port");
                        }

                        let frame_diff = present_stats.PresentCount- last_frame;
                        let time_diff = present_stats.SyncQPCTime - last_time;

                        if frame_diff > 1 {
                            println!("Missed Frames: {}", frame_diff - 1);
                        }

                        println!("Frame Time: {}ms (for {} frames)", time_diff as f64 / 10_000.0, frame_diff);
                        println!("Present Count: {}", present_stats.PresentRefreshCount - last_present_count);


                        last_time = present_stats.SyncQPCTime;
                        last_frame = present_stats.PresentCount;
                        last_present_count = present_stats.PresentRefreshCount;
                        running_frame = (last_frame % 20) as u8;

                    }
                    WindowEvent::CloseRequested => target.exit(),
                    _ => {}
                };
            }
        })
        .unwrap();
}

pub fn get_frame_stats(surface: &wgpu::Surface) -> DXGI_FRAME_STATISTICS {
    let mut present_stats: DXGI_FRAME_STATISTICS = DXGI_FRAME_STATISTICS::default();

    unsafe {
        surface.as_hal::<wgpu::core::api::Dx12,_,_>(
            |surface| {

                let sc = surface.unwrap().swap_chain();
                let sc = sc.as_ref().unwrap();
                let sc2 = sc.raw_swap_chain();
                sc2.GetFrameStatistics(&mut present_stats)
            }
        )
    };

    present_stats
}
pub fn main() {
    let event_loop = EventLoop::new().unwrap();
    #[allow(unused_mut)]
    let primary_monitor  = event_loop.available_monitors().last().expect("Failed to get primary monitor");
    let video_mode = primary_monitor.video_modes().next().expect("Failed to get video mode");
    println!("Video mode: {:?}", video_mode);
    let mut builder = winit::window::WindowBuilder::new().with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(primary_monitor))));
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        builder = builder.with_canvas(Some(canvas));
    }
    let window = builder.build(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}