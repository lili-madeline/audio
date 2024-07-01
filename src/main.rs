use alsa::pcm::{Access, Format, HwParams, State, IO};
use alsa::{Direction, ValueOr, PCM};
use async_executor::Executor;
use claxon::FlacReader;
use futures_lite::future;
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::{env, iter};
use wgpu::{
    Backends, Color, CommandEncoderDescriptor, Device, DeviceDescriptor, Features, Instance,
    InstanceDescriptor, Limits, LoadOp, Operations, PowerPreference, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, StoreOp, Surface,
    SurfaceConfiguration, TextureUsages, TextureViewDescriptor,
};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

macro_rules! dbg {
($( $x:expr ), +) => {

    if cfg!(debug_assertions) {
        std::dbg!($($x), +)
    } else {
            ($($x), +)
    }

}
}

#[derive(Default)]
struct App<'a> {
    window: Option<Arc<Window>>,
    render: Option<Arc<Mutex<Render<'a>>>>,
}

struct Render<'a> {
    surface: Surface<'a>,
    device: Device,
    queue: Queue,
    surface_configuration: SurfaceConfiguration,
    physical_size: PhysicalSize<u32>,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_visible(true))
                .unwrap(),
        );
        let executor = Executor::new();
        self.render = Some(future::block_on(
            executor.run(executor.spawn(ui_setup::<'_>(window.clone()))),
        ));
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let render = self.render.clone().unwrap();
        let mut render = render.lock().unwrap();
        match event {
            WindowEvent::Resized(s) => {
                render.physical_size = s;
                render.surface_configuration.height = s.height;
                render.surface_configuration.width = s.width;
                render
                    .surface
                    .configure(&render.device, &render.surface_configuration);
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let output = render.surface.get_current_texture().unwrap();
                let view = output
                    .texture
                    .create_view(&TextureViewDescriptor::default());
                let mut encoder = render
                    .device
                    .create_command_encoder(&CommandEncoderDescriptor {
                        label: Some("Render Encoder"),
                    });
                {
                    let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color {
                                    r: 0.9,
                                    g: 0.4,
                                    b: 0.4,
                                    a: 1.0,
                                }),
                                store: StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }
                render.queue.submit(iter::once(encoder.finish()));
                output.present();
            }
            _ => (),
        }
    }
}

async fn ui_setup<'a>(window: Arc<Window>) -> Arc<Mutex<Render<'a>>> {
    let physical_size = window.inner_size();
    let instance = Instance::new(InstanceDescriptor {
        backends: Backends::PRIMARY,
        ..Default::default()
    });
    let surface = instance.create_surface(window.clone()).unwrap();

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .unwrap();

    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                required_limits: Limits::default(),
            },
            None,
        )
        .await
        .unwrap();

    let surface_capabilities = surface.get_capabilities(&adapter);

    let surface_format = surface_capabilities
        .formats
        .iter()
        .find(|f| f.is_srgb())
        .copied()
        .unwrap_or(surface_capabilities.formats[0]);

    let surface_configuration = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: physical_size.width,
        height: physical_size.height,
        present_mode: surface_capabilities.present_modes[0],
        desired_maximum_frame_latency: 2,
        alpha_mode: surface_capabilities.alpha_modes[0],
        view_formats: vec![],
    };

    Arc::new(Mutex::new(Render {
        surface,
        device,
        queue,
        surface_configuration,
        physical_size,
    }))
}

fn run() {
    env_logger::init();

    let mut app = App::default();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app).unwrap();
}

fn main() {
    let mut track = None;
    if env::args().len() == 2 {
        track = env::args().last();
    }
    let reader = claxon::FlacReader::open(track.unwrap()).unwrap();
    let pcm = PCM::new("default", Direction::Playback, false).unwrap();

    dbg!(reader.streaminfo());

    let hwp = HwParams::any(&pcm).unwrap();
    hwp.set_channels(reader.streaminfo().channels).unwrap();
    hwp.set_rate(reader.streaminfo().sample_rate, ValueOr::Nearest)
        .unwrap();
    hwp.set_format(match reader.streaminfo().bits_per_sample {
        16 => Format::S16LE,
        24 => Format::S24LE,
        _ => panic!(),
    })
    .unwrap();
    hwp.set_access(Access::RWInterleaved).unwrap();
    pcm.hw_params(&hwp).unwrap();

    let io = pcm.io_bytes();

    let hwp = pcm.hw_params_current().unwrap();
    let swp = pcm.sw_params_current().unwrap();
    swp.set_start_threshold(hwp.get_buffer_size().unwrap())
        .unwrap();
    pcm.sw_params(&swp).unwrap();

    run();
    play_flac(reader, &pcm, &io);

    pcm.drain().unwrap();
}

fn play_flac(mut reader: FlacReader<File>, pcm: &PCM, io: &IO<u8>) {
    let mut blocks = 0;
    let mut vec_buf = Vec::<i32>::with_capacity(reader.streaminfo().max_block_size as usize);
    let mut block = reader.blocks().read_next_or_eof(vec_buf).unwrap();
    loop {
        #[cfg(debug_assertions)]
        dbg!(blocks);
        blocks += 1;
        match block {
            None => {
                break;
            }
            Some(b) => {
                let buffer = &b
                    .stereo_samples()
                    .flat_map(|i| [i.0.to_le_bytes(), i.1.to_le_bytes()])
                    .flatten()
                    .collect::<Vec<u8>>()[..];

                dbg!(io.writei(buffer).unwrap());

                vec_buf = b.into_buffer();
                block = reader.blocks().read_next_or_eof(vec_buf).unwrap();
                if pcm.state() != State::Running {
                    pcm.start().unwrap();
                };
            }
        }
    }
}
fn _play_pcm(pcm: &PCM, io: &IO<u8>) {
    let mut buffer = [0u8; 44100];
    let mut f = File::open("audio.pcm").unwrap();

    let mut time = 0;

    let mut data_read = f.read(&mut buffer).unwrap();
    dbg!(data_read, time);
    time += 1;

    while data_read >= 1024 {
        dbg!(io.writei(&buffer[..]).unwrap());
        data_read = f.read(&mut buffer).unwrap();
        dbg!(data_read, time);
        time += 1;
        if pcm.state() != State::Running {
            pcm.start().unwrap();
        };
    }
}
