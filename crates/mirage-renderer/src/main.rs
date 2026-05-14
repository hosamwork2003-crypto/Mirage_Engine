use winit::{event::*, event_loop::EventLoop, window::WindowBuilder};
use mirage_renderer::{MirageRenderer, SpawnRule, CameraUniform, NUM_ENTITIES, NUM_CHUNKS};
use std::time::{Instant, Duration};
use glam::{Vec3, Mat4};
use std::sync::Arc;

struct InputState { w: bool, a: bool, s: bool, d: bool, space: bool, lshift: bool }

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(WindowBuilder::new().with_title("Mirage 100K - Stable MCF").build(&event_loop).unwrap());
    let mut renderer = pollster::block_on(async { MirageRenderer::new(window.clone()).await });

    renderer.dispatch_spawn_rule(SpawnRule { entity_count: NUM_ENTITIES, seed: 42, spread: 4.5, speed: 0.005 });

    let mut cam_pos = Vec3::new(0.0, -8.0, 5.0); 
    let mut cam_yaw: f32 = 90.0;
    let mut cam_pitch: f32 = -30.0;
    let mut fov: f32 = 60.0;
    let mut input = InputState { w: false, a: false, s: false, d: false, space: false, lshift: false };
    let mut last_frame = Instant::now();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, .. }, .. } => {
                    let is_pressed = *state == ElementState::Pressed;
                    match physical_key {
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyW) => input.w = is_pressed,
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA) => input.a = is_pressed,
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyS) => input.s = is_pressed,
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyD) => input.d = is_pressed,
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Space) => input.space = is_pressed,
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ShiftLeft) => input.lshift = is_pressed,
                        _ => {}
                    }
                }
                WindowEvent::MouseWheel { delta: MouseScrollDelta::LineDelta(_, y), .. } => { fov = (fov - y * 3.0).clamp(5.0, 110.0); }
                WindowEvent::RedrawRequested => {
                    let dt = last_frame.elapsed().as_secs_f32();
                    if dt < 1.0 / 60.0 { std::thread::sleep(Duration::from_secs_f32(1.0 / 60.0 - dt)); }
                    last_frame = Instant::now();
                    let speed = 6.0 * dt;
                    let forward = Vec3::new(cam_yaw.to_radians().cos(), cam_yaw.to_radians().sin(), 0.0);
                    let right = Vec3::new(-cam_yaw.to_radians().sin(), cam_yaw.to_radians().cos(), 0.0);
                    if input.w { cam_pos += forward * speed; } if input.s { cam_pos -= forward * speed; }
                    if input.a { cam_pos -= right * speed; } if input.d { cam_pos += right * speed; }
                    if input.space { cam_pos.z += speed; } if input.lshift { cam_pos.z -= speed; }

                    // 🧠 الحل: التقاط موقع الكاميرا لاستخدامه في الـ Filter بسلام
                    let current_cam_pos = cam_pos;
                    let active_chunks: Vec<u32> = if current_cam_pos.distance(Vec3::ZERO) < 35.0 {
                        (0..NUM_CHUNKS).collect() // تشغيل كل الكتل لو قريبة
                    } else {
                        Vec::new() // 💤 خمول صفري لو الكاميرا بعيدة جداً
                    };
                    
                    let active_count = active_chunks.len() as u32;
                    if active_count > 0 { renderer.upload_active_chunks(&active_chunks); }
                    
                    let view = Mat4::look_at_rh(cam_pos, cam_pos + Vec3::new(cam_yaw.to_radians().cos(), cam_yaw.to_radians().sin(), cam_pitch.to_radians().sin()), Vec3::Z);
                    let proj = Mat4::perspective_rh(fov.to_radians(), 1280.0 / 720.0, 0.01, 1000.0);
                    renderer.update_camera(CameraUniform { view_proj: (proj * view).to_cols_array_2d() });
                    renderer.render(active_count).unwrap();
                }
                _ => {}
            },
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    }).unwrap();
}