#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use egui::{Context, FullOutput, Visuals};
use egui_glow::{Painter};
use egui_winit::{State};
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext};
use glutin::display::{Display, GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use raw_window_handle::{HasRawWindowHandle};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
use winit::window::{Window, WindowBuilder};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use winit::event::{Event};
use glow::{COLOR_BUFFER_BIT, Context as GLContext, HasContext};
use glutin_winit::{ApiPrefence, DisplayBuilder, finalize_window, GlWindow};
use tracing::instrument;
use winit::platform::run_return::EventLoopExtRunReturn;


fn main() {
    let mut event_loop = EventLoopBuilder::new()
        .build();

    let mut gui = GuiWindow::new(&event_loop);

    event_loop.run_return(move |event, _, control_flow| {
        gui.handle_events(&event);
        *control_flow = gui
            .next_repaint()
            .map(ControlFlow::WaitUntil)
            .unwrap_or(ControlFlow::Wait);
        if gui.close_requested {
            control_flow.set_exit();
        }
    });

}

pub struct GuiWindow {
    window: GraphicsWindow,
    painter: Painter,
    ctx: Context,
    state: State,
    next_repaint: Option<Instant>,
    close_requested: bool
}

impl GuiWindow {
    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Self {
        let window_builder = WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(LogicalSize { width: 800.0, height: 600.0 })
            //.with_window_icon(Some(crate::ui::WINDOW_ICON.clone()))
            .with_title("Headset Controller");

        let window = GraphicsWindow::new(window_builder, event_loop);

        let painter = window.make_painter();

        let ctx = Context::default();
        ctx.set_visuals(Visuals::light());

        let state = State::new(window.window());

        Self {
            window,
            painter,
            ctx,
            state,
            next_repaint: Some(Instant::now()),
            close_requested: false,
        }
    }

    pub fn next_repaint(&self) -> Option<Instant> {
        self.next_repaint
    }

    fn request_redraw(&mut self) {
        self.window.window().request_redraw();
    }

    #[instrument(skip_all)]
    pub fn handle_events<T>(&mut self, event: &Event<T>) {
        let id = self.window.window().id();
        match event {
            Event::RedrawRequested(window_id) if window_id == &id => self.redraw(),

            Event::WindowEvent { window_id, event} if window_id == &id => {
                use winit::event::WindowEvent;

                if let WindowEvent::CloseRequested = &event {
                    self.close_requested = true;
                }else if let WindowEvent::Resized(physical_size) = &event {
                    self.window.resize(*physical_size);
                } else if let WindowEvent::ScaleFactorChanged { new_inner_size, .. } = &event {
                    self.window.resize(**new_inner_size);
                }

                let event_response = self.state.on_event(&self.ctx, event);

                if event_response.repaint {
                    self.request_redraw();
                }
            }
            Event::LoopDestroyed => {
                //egui_glow.destroy();
            }
            //Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
            //    gl_window.window().request_redraw();
            //    redraw_requested = true;
            //},
            //Event::NewEvents(StartCause::Poll) => {
            //    if redraw_requested {
            //        gl_window.window().request_redraw();
            //    }
            //}
            Event::NewEvents(_) => {
                self
                    .next_repaint
                    .map(|t| Instant::now().checked_duration_since(t))
                    .is_some()
                    .then(|| self.request_redraw());
            }
            _ => (),
        }
    }

    fn redraw(&mut self) {
        let gui = |ctx: &Context| {
            static REPAINTS: AtomicU64 = AtomicU64::new(0);
            egui::SidePanel::left("my_side_panel").show(ctx, |ui| {
                ui.heading("Hello World!");

                if ui.button("Quit").clicked() {
                    //quit = true;
                    println!("Click!");
                }
                //ui.color_edit_button_rgb(&mut clear_color);
                ui.collapsing("Spinner", |ui| ui.spinner());
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(format!("draws: {}", REPAINTS.fetch_add(1, Ordering::Relaxed)));
                });
            });
        };


        let window = self.window.window();
        let raw_input = self.state.take_egui_input(window);
        let FullOutput {
            platform_output,
            repaint_after,
            mut textures_delta,
            shapes
        } = self.ctx.run(raw_input, gui);

        self.state.handle_platform_output(window, &self.ctx, platform_output);

        self.next_repaint = Instant::now().checked_add(repaint_after);
        {
            self.window.clear();

            for (id, image_delta) in textures_delta.set {
                self.painter.set_texture(id, &image_delta);
            }

            let clipped_primitives = self.ctx.tessellate(shapes);
            let dimensions: [u32; 2] = window.inner_size().into();
            self.painter
                .paint_primitives(dimensions, self.ctx.pixels_per_point(), &clipped_primitives);

            for id in textures_delta.free.drain(..) {
                self.painter.free_texture(id);
            }

            self.window.swap_buffers();
        }
    }

}

impl Drop for GuiWindow {
    #[instrument(skip_all)]
    fn drop(&mut self) {
        self.painter.destroy();
    }
}

pub struct GraphicsWindow {
    window: Window,
    gl_context: PossiblyCurrentContext,
    _gl_display: Display,
    gl_surface: Surface<WindowSurface>,
    gl: Arc<GLContext>
}

impl GraphicsWindow {
    #[instrument(skip_all, name = "gl_window_new")]
    pub fn new<T>(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<T>) -> Self {
        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false)
            .prefer_hardware_accelerated(None);

        tracing::debug!("trying to get gl_config");
        let (mut window, gl_config) = DisplayBuilder::new()
            .with_preference(ApiPrefence::FallbackEgl)
            .with_window_builder(Some(window_builder.clone()))
            .build(event_loop, template, |mut configs| {
                configs
                    .next()
                    .expect("failed to find a matching configuration for creating glutin config")
            })
            .expect("failed to create gl_config");

        tracing::debug!("found gl_config: {:?}", &gl_config);

        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());
        tracing::debug!("raw window handle: {:?}", raw_window_handle);
        let gl_display = gl_config.display();

        let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);

        let mut not_current_gl_context = Some(unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    tracing::debug!(
                        "failed to create gl_context with attributes: {:?}. retrying with fallback context attributes: {:?}",
                        &context_attributes,
                        &fallback_context_attributes
                    );
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        });

        let window = window.take().unwrap_or_else(|| {
            tracing::debug!("window doesn't exist yet. creating one now with finalize_window");
            finalize_window(event_loop, window_builder, &gl_config).expect("failed to finalize glutin window")
        });

        let attrs = window.build_surface_attributes(SurfaceAttributesBuilder::default());
        tracing::debug!("creating surface with attributes: {:?}", &attrs);
        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .expect("Failed to create window surface")
        };
        tracing::debug!("surface created successfully: {:?}.making context current", gl_surface);
        let gl_context = not_current_gl_context
            .take()
            .unwrap()
            .make_current(&gl_surface)
            .expect("Could not make context current");

        gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            .expect("Failed to activate vsync");

        let gl = Arc::new(unsafe {
            GLContext::from_loader_function(|s| {
                let s = std::ffi::CString::new(s).expect("failed to construct C string from string for gl proc address");
                gl_display.get_proc_address(&s)
            })
        });

        Self {
            window,
            gl_context,
            _gl_display: gl_display,
            gl_surface,
            gl
        }
    }

    #[instrument(skip_all)]
    pub fn make_painter(&self) -> Painter {
        Painter::new(self.gl.clone(), "", None).unwrap()
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    #[instrument(skip(self))]
    pub fn resize(&self, physical_size: PhysicalSize<u32>) {
        if physical_size.height != 0 && physical_size.width != 0 {
            self.gl_surface.resize(
                &self.gl_context,
                physical_size.width.try_into().unwrap(),
                physical_size.height.try_into().unwrap()
            );
        }
    }

    #[instrument(skip(self))]
    pub fn clear(&self) {
        let clear_color = [0.1, 0.1, 0.1];
        unsafe {
            self.gl
                .clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
            self.gl.clear(COLOR_BUFFER_BIT);
        }
    }

    #[instrument(skip(self))]
    pub fn swap_buffers(&self) {
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .expect("Failed to swap buffers")
    }
}


/*
mod audio;
mod config;
mod debouncer;
mod devices;
mod notification;
mod renderer;
mod tray;
mod ui;
mod util;

use std::ops::Not;
use std::sync::{Arc};
use std::time::{Duration, Instant};

use color_eyre::Result;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::run_return::EventLoopExtRunReturn;
use tokio::runtime::Builder;
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use futures_lite::StreamExt;
use parking_lot::Mutex;
use tokio::spawn;

use crate::audio::AudioSystem;
use crate::config::{Config, EqualizerConfig, HeadsetConfig, CLOSE_IMMEDIATELY, START_QUIET, PRINT_UDEV_RULES};
use crate::debouncer::{Action, ActionReceiver, ActionProxy, Debouncer, debouncer, ActionSender};
use crate::devices::{BatteryLevel, BoxedDevice, Device, DeviceList, DeviceUpdate, generate_udev_rules};
use crate::renderer::EguiWindow;
use crate::tray::{AppTray, TrayEvent};

fn main() -> Result<()> {
    if *PRINT_UDEV_RULES { return Ok(println!("{}", generate_udev_rules()?)); }
    color_eyre::install()?;
    //let logfile = Mutex::new(log_file());
    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(Targets::new().with_default(LevelFilter::TRACE))
        .with(layer().without_time())
        //.with(layer().with_ansi(false).with_writer(logfile))
        .init();


    let span = tracing::info_span!("init").entered();

    let config = Arc::new(Mutex::new(Config::load()?));

    let mut event_loop = EventLoop::new();

    let mut audio_system = AudioSystem::new();

    let device_manager = Arc::new(Mutex::new(DeviceList::empty()));
    let device: Arc<Mutex<Option<BoxedDevice>>> = Arc::new(Mutex::new(None));

    //let tray = AppTray::new(&event_loop);

    let mut window: Option<EguiWindow> = START_QUIET.not().then(|| EguiWindow::new(&event_loop));

    let (mut action_sender, action_receiver) = debouncer();
    let mut debouncer = Debouncer::new();

    action_sender.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip, Action::UpdateTray]);

    span.exit();

    let runtime = {
        let device_manager = device_manager.clone();
        let device = device.clone();
        let config = config.clone();
        std::thread::spawn(move || {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to start runtime")
                .block_on(action_handler(action_receiver, device_manager, device, config))
        })
    };

    event_loop.run_return(move |event, event_loop, control_flow| {
        if window
            .as_mut()
            .map(|w| {
                w.handle_events(&event, |egui_ctx| {
                    let device = device.lock();
                    let mut config = config.lock();
                    match device.as_ref() {
                        Some(device) => ui::config_ui(
                            egui_ctx,
                            &mut action_sender,
                            &mut config,
                            device.as_ref(),
                            device_manager.as_ref(),
                            &mut audio_system
                        ),
                        None => ui::no_device_ui(egui_ctx, &mut action_sender)
                    }
                })
            })
            .unwrap_or(false)
        {
            action_sender.force(Action::SaveConfig);
            window.take();
            if *CLOSE_IMMEDIATELY {
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            /*
            Event::MenuEvent { menu_id, .. } => {
                let _span = tracing::info_span!("tray_menu_event").entered();
                match tray.handle_event(menu_id) {
                    Some(TrayEvent::Open) => {
                        audio_system.refresh_devices();
                        match &mut window {
                            None => window = Some(EguiWindow::new(event_loop)),
                            Some(window) => {
                                window.focus();
                            }
                        }
                    }
                    Some(TrayEvent::Quit) => {
                        *control_flow = ControlFlow::Exit;
                    }
                    Some(TrayEvent::Profile(id)) => {
                        let _span = tracing::info_span!("profile_change", id).entered();
                        let device = device.lock();
                        if let Some(device) = device.as_ref() {
                            let mut config = config.lock();
                            let headset = config.get_headset(device.name());
                            if id as u32 != headset.selected_profile_index {
                                let len = headset.profiles.len();
                                if id < len {
                                    headset.selected_profile_index = id as u32;
                                    submit_profile_change(&action_sender);
                                    action_sender.submit_all([Action::SaveConfig, Action::UpdateTray]);
                                } else {
                                    tracing::warn!(len, "Profile id out of range")
                                }
                            } else {
                                tracing::trace!("Profile already selected");
                            }
                        }
                    }
                    _ => {}
                }
            }
             */
            _ => ()
        }
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            let next_update = window.as_ref().and_then(|w| w.next_repaint());
            *control_flow = match next_update {
                Some(next_update) => match next_update <= Instant::now() {
                    true => ControlFlow::Poll,
                    false => ControlFlow::WaitUntil(next_update)
                },
                None => ControlFlow::Wait
            };
        }
    });

    runtime.join().unwrap();
    Ok(())
}

async fn action_handler(
    mut action_receiver: ActionReceiver,
    device_manager: Arc<Mutex<DeviceList>>,
    device: Arc<Mutex<Option<BoxedDevice>>>,
    config: Arc<Mutex<Config>>) {

    let (device_update_sender, device_update_receiver) = flume::unbounded();

    spawn(async move {
        while let Ok(update) = device_update_receiver.recv_async().await {
            println!("DeviceUpdate: {:?}", update);
            //match event {
            //    DeviceUpdate::ConnectionChanged | DeviceUpdate::BatteryLevel => action_sender.submit(Action::UpdateDeviceStatus),
            //    DeviceUpdate::DeviceError(err) => tracing::error!("The device return an error: {}", err),
            //    DeviceUpdate::ChatMixChanged => {}
            //}
        }
    });

    *device_manager.lock() = DeviceList::new()
        .await
        .unwrap_or_else(|err| {
            tracing::warn!("Failed to enumerate devices: {:?}", err);
            DeviceList::empty()
        });
    *device.lock() = device_manager
        .lock()
        .find_preferred_device(&config.lock().preferred_device, device_update_sender.clone())
        .await;

    let mut last_connected = false;
    let mut last_battery = Default::default();

    while let Some(action) = action_receiver.next().await {
        let _span = tracing::info_span!("debouncer_event", ?action).entered();
        tracing::trace!("Processing event");
        match action {
            Action::UpdateDeviceStatus => {
                let device = device.lock();
                if let Some(device) = device.as_ref() {
                    let current_connection = device.is_connected();
                    let current_battery = device.get_battery_status();
                    if current_connection != last_connected {
                        let msg = build_notification_text(current_connection, &[current_battery, last_battery]);
                        notification::notify(device.name(), &msg, Duration::from_secs(2))
                            .unwrap_or_else(|err| tracing::warn!("Can not create notification: {:?}", err));
                        action_receiver.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip]);
                        action_receiver.force(Action::UpdateSystemAudio);
                        last_connected = current_connection;
                    }
                    if last_battery != current_battery {
                        action_receiver.submit(Action::UpdateTrayTooltip);
                        last_battery = current_battery;
                    }
                }
            }
            Action::RefreshDeviceList => {
                *device.lock() = None;
                let list = DeviceList::new()
                    .await
                    .unwrap_or_else(|err| {
                        tracing::warn!("Failed to refresh devices: {}", err);
                        DeviceList::empty()
                    });
                *device_manager.lock() = list;
            },
            Action::SwitchDevice => {
                let mut device = device.lock();
                let preferred_device = config.lock().preferred_device.clone();
                if preferred_device != device.as_ref().map(|d| d.name().to_string()) {
                    *device = device_manager
                            .lock()
                            .find_preferred_device(&preferred_device, device_update_sender.clone())
                            .await;
                    action_receiver.submit_full_change();
                    action_receiver.submit_all([Action::UpdateTray, Action::UpdateTrayTooltip]);
                } else {
                    tracing::debug!("Preferred device is already active")
                }
            }
            Action::UpdateSystemAudio => {
                //TODO REIMPLEMENT
                //let device = device.lock();
                //if let Some(device) = device.as_ref() {
                //    let mut config = config.lock();
                //    let headset = config.get_headset(device.name());
                //    audio_system.apply(&headset.os_audio, device.is_connected())
                //}
            }
            Action::SaveConfig => {
                config
                    .lock()
                    .save()
                    .unwrap_or_else(|err| tracing::warn!("Could not save config: {:?}", err));
            }
            Action::UpdateTray => {
                //TODO REIMPLEMENT
                //let mut config = config.lock();
                //update_tray(&mut tray, &mut config, device.lock().as_ref().map(|d| d.name()))
            },
            Action::UpdateTrayTooltip => {
                //TODO REIMPLEMENT
                //update_tray_tooltip(&mut tray, &device.lock())
            },
            action => {
                let device = device.lock();
                if let Some(device) = device.as_ref() {
                    let mut config = config.lock();
                    let headset = config.get_headset(device.name());
                    apply_config_to_device(action, device.as_ref(), headset)
                }
            }
        }
    }
}

#[instrument(skip_all, fields(name = %device.name()))]
fn apply_config_to_device(action: Action, device: &dyn Device, headset: &mut HeadsetConfig) {
    if device.is_connected() {
        match action {
            Action::UpdateSideTone => {
                if let Some(sidetone) = device.get_side_tone() {
                    let _span = tracing::info_span!("sidetone").entered();
                    sidetone.set_level(headset.selected_profile().side_tone);
                }
            }
            Action::UpdateEqualizer => {
                if let Some(equalizer) = device.get_equalizer() {
                    let _span = tracing::info_span!("equalizer").entered();
                    let levels = match headset.selected_profile().equalizer.clone() {
                        EqualizerConfig::Preset(i) => equalizer
                            .presets()
                            .get(i as usize)
                            .expect("Unknown preset")
                            .1
                            .to_vec(),
                        EqualizerConfig::Custom(levels) => levels
                    };
                    equalizer.set_levels(&levels);
                }
            }
            Action::UpdateMicrophoneVolume => {
                if let Some(mic_volume) = device.get_mic_volume() {
                    let _span = tracing::info_span!("mic_volume").entered();
                    mic_volume.set_level(headset.selected_profile().microphone_volume);
                }
            }
            Action::UpdateVolumeLimit => {
                if let Some(volume_limiter) = device.get_volume_limiter() {
                    let _span = tracing::info_span!("volume_limiter").entered();
                    volume_limiter.set_enabled(headset.selected_profile().volume_limiter);
                }
            }
            Action::UpdateInactiveTime => {
                if let Some(inactive_time) = device.get_inactive_time() {
                    let _span = tracing::info_span!("inactive time").entered();
                    inactive_time.set_inactive_time(headset.inactive_time);
                }
            }
            Action::UpdateMicrophoneLight => {
                if let Some(mic_light) = device.get_mic_light() {
                    let _span = tracing::info_span!("mic_light").entered();
                    mic_light.set_light_strength(headset.mic_light);
                }
            }
            Action::UpdateAutoBluetooth => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config.set_auto_enabled(headset.auto_enable_bluetooth);
                }
            }
            Action::UpdateBluetoothCall => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config.set_call_action(headset.bluetooth_call);
                }
            }
            _ => tracing::warn!("{:?} is not related to the device", action)
        }
    }
}

#[instrument(skip_all)]
pub fn update_tray(tray: &mut AppTray, config: &mut Config, device_name: Option<&str>) {
    match device_name {
        None => {
            tray.build_menu(0, |_| ("", false));
        }
        Some(device_name) => {
            let headset = config.get_headset(device_name);
            let selected = headset.selected_profile_index as usize;
            let profiles = &headset.profiles;
            tray.build_menu(profiles.len(), |id| (profiles[id].name.as_str(), id == selected));
        }
    }
}

#[instrument(skip_all)]
pub fn update_tray_tooltip(tray: &mut AppTray, device: &Option<BoxedDevice>) {
    match device {
        None => {
            tray.set_tooltip("No Device");
        }
        Some(device) => {
            let name = device.name().to_string();
            let tooltip = match device.is_connected() {
                true => match device.get_battery_status() {
                    Some(BatteryLevel::Charging) => format!("{name}\nBattery: Charging"),
                    Some(BatteryLevel::Level(level)) => format!("{name}\nBattery: {level}%"),
                    _ => format!("{name}\nConnected")
                },
                false => format!("{name}\nDisconnected")
            };
            tray.set_tooltip(&tooltip);
        }
    }
    tracing::trace!("Updated tooltip");
}

fn build_notification_text(connected: bool, battery_levels: &[Option<BatteryLevel>]) -> String {
    let msg = match connected {
        true => "Connected",
        false => "Disconnected"
    };
    battery_levels
        .iter()
        .filter_map(|b| match b {
            Some(BatteryLevel::Level(l)) => Some(*l),
            _ => None
        })
        .min()
        .map(|level| format!("{} (Battery: {}%)", msg, level))
        .unwrap_or_else(|| msg.to_string())
}
*/