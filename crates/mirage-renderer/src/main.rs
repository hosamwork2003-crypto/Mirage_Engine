use winit::event::{Event, WindowEvent, ElementState};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

use mirage_renderer::{
    MirageRenderer,
    CameraUniform,
    NUM_CHUNKS,
};

use glam::{Vec3, Mat4};

use std::sync::Arc;
use std::sync::mpsc::{
    channel,
    Receiver,
    Sender,
};

use mirage_core::pool::RuntimeDirectory;
use mirage_core::runtime::ChunkState;
use mirage_core::oasis::OasisManager;

struct InputState {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    k: bool,
}

#[derive(Clone, Copy, Default)]
struct ChunkMetadata {
    state: ChunkState,
}

fn main() {

    let event_loop =
        EventLoop::new().unwrap();

    let window =
        Arc::new(
            WindowBuilder::new()
                .with_title("Mirage Engine")
                .build(&event_loop)
                .unwrap(),
        );

    let mut renderer =
        pollster::block_on(
            MirageRenderer::new(window.clone()),
        );

    let mut directory =
        RuntimeDirectory::new(
            NUM_CHUNKS as usize,
        );

    let oasis =
        Arc::new(
            OasisManager::new(),
        );

    let (tx, rx):
        (
            Sender<(u32, Vec<u8>)>,
            Receiver<(u32, Vec<u8>)>,
        ) = channel();

    let mut input_state =
        InputState {
            w: false,
            a: false,
            s: false,
            d: false,
            k: false,
        };

    let mut cam_pos =
        Vec3::new(
            0.0,
            0.0,
            50.0,
        );

    let mut last_cam_pos =
        cam_pos;

    let _ =
        event_loop.run(
            move |event, target| {

                match event {

                    Event::WindowEvent {
                        event:
                            WindowEvent::CloseRequested,
                        ..
                    } => {

                        target.exit();
                    }

                    Event::WindowEvent {
                        event:
                            WindowEvent::KeyboardInput {
                                event,
                                ..
                            },
                        ..
                    } => {

                        let pressed =
                            event.state
                                == ElementState::Pressed;

                        if let winit::keyboard::Key::Character(c)
                            = &event.logical_key
                        {
                            match c.as_str() {

                                "w" => {
                                    input_state.w = pressed;
                                }

                                "a" => {
                                    input_state.a = pressed;
                                }

                                "s" => {
                                    input_state.s = pressed;
                                }

                                "d" => {
                                    input_state.d = pressed;
                                }

                                "k" => {
                                    input_state.k = pressed;
                                }

                                _ => {}
                            }
                        }
                    }

                    Event::AboutToWait => {

                        //
                        // CAMERA MOVEMENT
                        //
                        if input_state.w {
                            cam_pos.z -= 1.0;
                        }

                        if input_state.s {
                            cam_pos.z += 1.0;
                        }

                        if input_state.a {
                            cam_pos.x -= 1.0;
                        }

                        if input_state.d {
                            cam_pos.x += 1.0;
                        }

                        let cam_vel =
                            cam_pos - last_cam_pos;

                        last_cam_pos =
                            cam_pos;

                        //
                        // STREAMED CHUNKS
                        //
                        while let Ok((idx, data))
                            = rx.try_recv()
                        {
                            renderer
                                .upload_chunk_to_vram(
                                    idx,
                                    &data,
                                );

                            directory
                                .chunk_runtime_states
                                [idx as usize]
                                = ChunkState::Resident;
                        }

                        let mut active_chunks =
                            Vec::new();

                        //
                        // CHUNK VISIBILITY
                        //
                        for i in 0..NUM_CHUNKS as usize {

                            let chunk_pos =
                                Vec3::new(
                                    (i % 25) as f32 * 64.0,
                                    0.0,
                                    (i / 25) as f32 * 64.0,
                                );

                            let dist =
                                cam_pos.distance(chunk_pos);

                            //
                            // HOT
                            //
                            if dist < 60.0 {

                                directory
                                    .chunk_runtime_states[i]
                                    = ChunkState::Hot;

                                active_chunks
                                    .push(i as u32);
                            }

                            //
                            // PREDICTIVE
                            //
                            else if dist < 120.0 {

                                if directory
                                    .chunk_runtime_states[i]
                                    == ChunkState::Dormant
                                {
                                    let tx_c =
                                        tx.clone();

                                    let oasis_c =
                                        oasis.clone();

                                    std::thread::spawn(
                                        move || {

                                            let data =
                                                oasis_c
                                                    .load_chunk_data(
                                                        0,
                                                        i as u32,
                                                    );

                                            let _ =
                                                tx_c.send(
                                                    (
                                                        i as u32,
                                                        data,
                                                    ),
                                                );
                                        },
                                    );

                                    directory
                                        .chunk_runtime_states[i]
                                        = ChunkState::Predictive;
                                }

                                else if directory
                                    .chunk_runtime_states[i]
                                    == ChunkState::Resident
                                {
                                    active_chunks
                                        .push(i as u32);
                                }
                            }
                        }

                        //
                        // GPU STATE UPLOAD
                        //
                        let raw_states =
                            directory
                                .get_raw_states();

                        renderer
                            .update_states_buffer(
                                &raw_states,
                            );

                        if !active_chunks.is_empty() {

                            renderer
                                .upload_active_chunks(
                                    &active_chunks,
                                );
                        }

                        //
                        // CAMERA
                        //
                        let view =
                            Mat4::look_at_rh(
                                cam_pos,
                                cam_pos
                                    + Vec3::new(
                                        0.0,
                                        0.0,
                                        -1.0,
                                    ),
                                Vec3::Y,
                            );

                        let proj =
                            Mat4::perspective_rh(
                                45.0f32.to_radians(),
                                1.2,
                                0.1,
                                1000.0,
                            );

                        renderer.update_camera(
                            CameraUniform {
                                view_proj:
                                    (proj * view)
                                        .to_cols_array_2d(),
                            },
                        );

                        //
                        // RENDER
                        //
                        renderer.reset_draw_count();
                        let _ =
                            renderer.render(
                                active_chunks.len()
                                    as u32,
                            );

                        let _ =
                            cam_vel;
                    }

                    _ => {}
                }
            },
        );
}