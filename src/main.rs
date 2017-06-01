#[macro_use] extern crate nix;
extern crate input;
extern crate libc;
extern crate libudev_sys;
extern crate time;

mod uinput;

use input::{AsRaw, Libinput, LibinputInterface};
use input::Event::{Keyboard, Pointer};
use input::event::Event;
use input::event::KeyboardEvent::Key;
use input::event::PointerEvent::Motion;
use input::event::keyboard::{KeyboardEventTrait, KeyState};
use input::event::pointer::PointerEventTrait;
use libc::{c_char, c_int, c_void};
use uinput::UInput;

const SEAT_NAME: &'static str = "seat0";
const RECORD_KEY: uinput::Key = uinput::Key::Esc;
const REPLAY_KEY: uinput::Key = uinput::Key::F2;
static INTERFACE: LibinputInterface = LibinputInterface {
    open_restricted: Some(open_restricted),
    close_restricted: Some(close_restricted),
};

extern fn open_restricted(path: *const c_char, flags: c_int, user_data: *mut c_void) -> c_int {
    // We avoid creating a Rust File because that requires abiding by Rust lifetimes.
    unsafe {
        let fd = ::libc::open(path, flags);
        if fd < 0 {
            println!("open_restricted failed.");
        }
        fd
    }
}

extern fn close_restricted(fd: c_int, user_data: *mut c_void) {
    unsafe {
        libc::close(fd);
    }
}

unsafe fn libinput_from_udev() -> Libinput {
    let udev = libudev_sys::udev_new();
    if udev.is_null() {
        panic!("Could not create udev context.");
    }

    let mut libinput = Libinput::new_from_udev::<&str>(INTERFACE, None, udev as *mut _);
    if libinput.as_raw().is_null() {
        panic!("Failed to create libinput context.");
    }

    libinput.udev_assign_seat(SEAT_NAME).ok();

    libudev_sys::udev_unref(udev);

    libinput
}

fn replay_events(events: &Vec<Event>, uinput: &mut UInput) {
    println!("Replay!");
    let mut prev_event_time = 0;

    for e in events {
        // Not ideal, but can't get time on generic Event.
        // Each event should assign this value to msec time
        let mut time = prev_event_time;

        match e {
            &Keyboard(Key(ref key_event)) => {
                let key = key_event.key();
                time = key_event.time_usec() / 1000;

                match key_event.key_state() {
                    KeyState::Pressed => {
                        uinput.key_press(uinput::Key::from(key as u8));
                    },
                    KeyState::Released => {
                        uinput.key_release(uinput::Key::from(key as u8));
                    },
                }
            },
            &Pointer(Motion(ref motion_event)) => {
                time = motion_event.time_usec() / 1000;

                uinput.rel_x(motion_event.dx() as i32);
                uinput.rel_y(motion_event.dy() as i32);
            },
            _ => {},
        }
        // Sleep for event delta time then send event
        if prev_event_time != 0 && prev_event_time != time {
            std::thread::sleep(std::time::Duration::from_millis(time - prev_event_time));
        }
        prev_event_time = time;
        uinput.sync();
    }
}

fn mouse_to_home(home: (f64, f64), curr: (f64, f64), uinput: &mut UInput) {
    println!("Attempting to move to {:?} from {:?}", home, curr);
    let delta_x = home.0 - curr.0;
    let delta_y = home.1 - curr.1;
    println!("Moving {} x and {} y", delta_x as i32, delta_y as i32);

    uinput.rel_x(delta_x as i32);
    uinput.rel_y(delta_y as i32);
    uinput.sync();
}

fn main() {
    let mut libinput = unsafe { libinput_from_udev() };
    let mut uinput = uinput::UInput::new();
    let mut event_store = Vec::new();

    let mut recording = false;
    let mut pointer_record_start = (0_f64, 0_f64);
    let mut pointer_delta = (0_f64, 0_f64); // total accumulated change in mouse since start of program

    loop {
        if let Err(_) = libinput.dispatch() {
            panic!("libinput dispatch failed.");
        }

        while let Some(event) = libinput.next() {
            match event {
                Keyboard(Key(key_event)) => {
                    let key = key_event.key();
                    let key_state = key_event.key_state();

                    // Perhaps check if prev_event_time was 0;

                    match uinput::Key::from(key as u8) {
                        RECORD_KEY => {
                            if key_state == KeyState::Released {
                                recording = !recording;

                                if recording {
                                    // Flush the event_store
                                    event_store.clear();
                                    pointer_record_start = pointer_delta;
                                    println!("Now recording.");
                                } else {
                                    println!("Stopped recording.");
                                }
                            }
                        },
                        REPLAY_KEY => {
                            if key_state == KeyState::Released {
                                recording = false;
                                mouse_to_home(pointer_record_start, pointer_delta, &mut uinput);

                                libinput.suspend();
                                replay_events(&event_store, &mut uinput);
                                libinput.resume();
                            }
                        },
                        _ => if recording {
                            event_store.push(Keyboard(Key(key_event)));
                        },
                    }

                    //println!("Key {} {:?} [+{}ms]", key, key_state, (time - prev_event_time) / 1000);
                },
                Pointer(Motion(pointer_motion_event)) => {
                    pointer_delta.0 += pointer_motion_event.dx();
                    pointer_delta.1 += pointer_motion_event.dy();

                    if recording {
                        event_store.push(Pointer(Motion(pointer_motion_event)));
                    }

                    println!("Total {:?}", pointer_delta);
                },
                e => {
                    if recording {
                        event_store.push(e);
                    }
                },
            }
        }
    }
}
