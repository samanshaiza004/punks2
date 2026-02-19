use std::sync::Arc;
use std::time::Instant;

use imgui::FontSource;
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::WinitPlatform;
use pollster::block_on;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::Window,
};

use punks_browser::SampleBrowser;
use punks_ui::BrowserPanel;

struct GpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
}

struct ImguiState {
    context: imgui::Context,
    platform: WinitPlatform,
    renderer: Renderer,
    last_frame: Instant,
    last_cursor: Option<imgui::MouseCursor>,
}

struct AppWindow {
    gpu: GpuState,
    imgui: ImguiState,
    browser: SampleBrowser,
    panel: BrowserPanel,
}

impl AppWindow {
    fn new(event_loop: &ActiveEventLoop) -> Self {
        let gpu = Self::init_gpu(event_loop);
        let imgui = Self::init_imgui(&gpu);
        let browser = SampleBrowser::new().expect("failed to initialize audio engine");
        let panel = BrowserPanel::new();

        AppWindow {
            gpu,
            imgui,
            browser,
            panel,
        }
    }

    fn init_gpu(event_loop: &ActiveEventLoop) -> GpuState {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let size = LogicalSize::new(800.0, 600.0);
        let attributes = Window::default_attributes()
            .with_inner_size(size)
            .with_title("punks2 — Sample Browser");
        let window = Arc::new(event_loop.create_window(attributes).unwrap());

        let phys_size = window.inner_size();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter found");

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("failed to create GPU device");

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: phys_size.width.max(1),
            height: phys_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
        };
        surface.configure(&device, &surface_config);

        GpuState {
            device,
            queue,
            window,
            surface,
            surface_config,
        }
    }

    fn init_imgui(gpu: &GpuState) -> ImguiState {
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);

        let mut platform = WinitPlatform::new(&mut context);
        platform.attach_window(
            context.io_mut(),
            &gpu.window,
            imgui_winit_support::HiDpiMode::Default,
        );

        let hidpi = gpu.window.scale_factor();
        let font_size = (14.0 * hidpi) as f32;
        context.io_mut().font_global_scale = (1.0 / hidpi) as f32;

        context.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

        let renderer_config = RendererConfig {
            texture_format: gpu.surface_config.format,
            ..Default::default()
        };
        let renderer = Renderer::new(&mut context, &gpu.device, &gpu.queue, renderer_config);

        ImguiState {
            context,
            platform,
            renderer,
            last_frame: Instant::now(),
            last_cursor: None,
        }
    }
}

#[derive(Default)]
struct App {
    window: Option<AppWindow>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(AppWindow::new(event_loop));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let app = match self.window.as_mut() {
            Some(w) => w,
            None => return,
        };
        let im = &mut app.imgui;

        match &event {
            WindowEvent::Resized(size) => {
                app.gpu.surface_config.width = size.width.max(1);
                app.gpu.surface_config.height = size.height.max(1);
                app.gpu
                    .surface
                    .configure(&app.gpu.device, &app.gpu.surface_config);
            }

            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput { event, .. } => {
                if let Key::Named(NamedKey::Escape) = event.logical_key {
                    if event.state.is_pressed() {
                        event_loop.exit();
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                im.context.io_mut().update_delta_time(now - im.last_frame);
                im.last_frame = now;

                let frame = match app.gpu.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(e) => {
                        log::error!("dropped frame: {e:?}");
                        return;
                    }
                };

                im.platform
                    .prepare_frame(im.context.io_mut(), &app.gpu.window)
                    .expect("failed to prepare imgui frame");

                let ui = im.context.frame();

                // Full-window imgui panel
                let display_size = ui.io().display_size;
                ui.window("Sample Browser")
                    .position([0.0, 0.0], imgui::Condition::Always)
                    .size(display_size, imgui::Condition::Always)
                    .no_decoration()
                    .movable(false)
                    .build(|| {
                        app.panel.draw(ui, &mut app.browser);
                    });

                if im.last_cursor != ui.mouse_cursor() {
                    im.last_cursor = ui.mouse_cursor();
                    im.platform.prepare_render(ui, &app.gpu.window);
                }

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder = app
                    .gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let clear_color = wgpu::Color {
                    r: 0.08,
                    g: 0.08,
                    b: 0.10,
                    a: 1.0,
                };

                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    im.renderer
                        .render(
                            im.context.render(),
                            &app.gpu.queue,
                            &app.gpu.device,
                            &mut rpass,
                        )
                        .expect("imgui render failed");
                }

                app.gpu.queue.submit(Some(encoder.finish()));
                frame.present();
            }

            _ => {}
        }

        im.platform.handle_event::<()>(
            im.context.io_mut(),
            &app.gpu.window,
            &Event::WindowEvent { window_id, event },
        );
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = self.window.as_mut() {
            app.gpu.window.request_redraw();
            app.imgui.platform.handle_event::<()>(
                app.imgui.context.io_mut(),
                &app.gpu.window,
                &Event::AboutToWait,
            );
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::default()).unwrap();
}
