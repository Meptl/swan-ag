extern crate argparse;
extern crate input;
extern crate libc;
extern crate libudev_sys;
#[macro_use] extern crate nix;
extern crate time;

mod uinput;

use argparse::{ArgumentParser, Store, StoreOption, StoreTrue};
use input::{AsRaw, Libinput, LibinputInterface};
use input::Event::{Keyboard, Pointer};
use input::event::Event;
use input::event::KeyboardEvent::Key;
use input::event::PointerEvent::{Button, Motion};
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

/// Command line options for this program.
struct Options {
    speed: f64,
    record_delay: Option<f64>,
    record_length: Option<f64>
}

impl Default for Options {
    fn default() -> Self {
        Options {
            speed: 1.0,
            record_delay: None,
            record_length: None,
        }
    }
}

/// Create a Libinput struct from udev
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

/// Replay events in the event store.
/// Modifies the pointer position.
fn replay_events(options: &Options, events: &Vec<Event>, uinput: &mut UInput) {
    println!("Replay!");
    let mut prev_event_time = 0;
    let mut pointer_err = (0_f64, 0_f64); // Total accumulated positional error

    for e in events {
        // Not ideal, but can't get time on generic Event (Devices don't have a time)
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
                // This assumes that the units from libinput are the same as that of the uinput
                // device. This is WRONG and doesn't work for some devices. (i.e. my touchpad)
                // Using accelerated data makes touchpads work slightly better but makes worse
                // mouse control.
                let x = motion_event.dx_unaccelerated();
                let y = motion_event.dy_unaccelerated();
                //println!("Rel {} {}", x, y);
                time = motion_event.time_usec() / 1000;

                uinput.rel_x(x as i32);
                uinput.rel_y(y as i32);

                // Though unaccelerated data is typically integers.
                pointer_err.0 += x.fract();
                pointer_err.1 += y.fract();

                if pointer_err.0.abs() > 1.0 {
                    uinput.rel_x(pointer_err.0 as i32);      // Sends 1 or -1
                    pointer_err.0 -= pointer_err.0.trunc(); // Subtracts 1 or -1.
                }
                if pointer_err.1.abs() > 1.0 {
                    uinput.rel_y(pointer_err.1 as i32);             // Sends 1 or -1
                    pointer_err.1 -= pointer_err.1.trunc(); // Subtracts 1 or -1.
                }
            },
            &Pointer(Button(ref button_event)) => {
                let button = button_event.button();
                let value = button_event.seat_button_count();

                match (button, value) {
                    (0x110, 0) => uinput.btn_left_release(),
                    (0x110, 1) => uinput.btn_left_press(),
                    (0x111, 0) => uinput.btn_right_release(),
                    (0x111, 1) => uinput.btn_right_press(),
                    _ => println!("Unimplemented button event!")
                }
            },
            _ => {},
        }
        // Sleep for event delta time then send event
        // Sometimes events become unordered and time is off.
        if options.speed != 0.0 && prev_event_time != 0 && prev_event_time < time {
            let delay_ms = (time - prev_event_time) as f64 / options.speed;
            std::thread::sleep(std::time::Duration::from_millis(delay_ms as u64));
        }
        prev_event_time = time;

        // For some events like motion, this is not necessary.
        uinput.sync();
    }
}

/// Return the options struct based on command line arguments.
fn parse_args() -> Options {
    let mut options = Options::default();
    let mut instant = false;
    {
        let mut ap = ArgumentParser::new();
        ap.set_description("Record input events and replay them. Use the --delayed and --length option for buttonless recording.");
        ap.refer(&mut options.record_delay)
          .add_option(&["-d", "--delayed"], StoreOption,
                      "Start recording after a number of seconds");
        ap.refer(&mut instant)
          .add_option(&["-i", "--instant"], StoreTrue,
                      "Replay events with no delay between them");
        ap.refer(&mut options.record_length)
          .add_option(&["-l", "--length"], StoreOption,
                      "Recordings will stop after a number of seconds");
        /*
        ap.refer(&mut options.no_flush)
          .add_option(&["-n", "--no-flush"], StoreTrue,
                      "Don't flush stored events between recordings");
        ap.refer(&mut options.record_key)
          .add_option(&["-r", "--record-key"], Store,
                      "User specified replay key");
        ap.refer(&mut options.no_flush)
          .add_option(&["-p", "--replay-key"], Store,
                      "User specified record key");
                      */
        ap.refer(&mut options.speed)
          .add_option(&["-s", "--speed"], Store,
                      "Replay speed modifier (default: 1.0)");
        ap.parse_args_or_exit();
    }

    if instant {
        options.speed = 0.0;
    }

    options
}

/// Returns the secs and nsecs of the given floating point seconds.
fn f64_sec(duration: f64) -> (u64, u32) {
    if !duration.is_finite() || duration.is_sign_negative() {
        panic!("Invalid delay value: {}", duration);
    }

    let secs = duration as u64;
    let nsecs = (duration.fract() * 1000000000.0) as u32;

    (secs, nsecs)
}

/// Sleep for number of seconds in floating point
fn sleep_secs(duration: f64) {
    let (secs, nsecs) = f64_sec(duration);

    let duration = std::time::Duration::new(secs, nsecs as u32);

    std::thread::sleep(duration);
}

/// Check if time since given Timespec is larger than given number of seconds in floating point
fn time_has_elapsed(start_time: time::Timespec, duration: f64) -> bool {
    let (secs, nsecs) = f64_sec(duration);

    // duration is in floating point seconds
    let time_elapsed = (time::get_time() - start_time).to_std().expect("duration out of range.");

    time_elapsed.as_secs() > secs && time_elapsed.subsec_nanos() > nsecs
}

fn start_recording(event_store: &mut Vec<Event>, recording: &mut bool, start_time: &mut time::Timespec) {
    event_store.clear();
    *start_time = time::get_time();
    *recording = true;
    println!("Started recording!");
}

fn stop_recording(recording: &mut bool) {
    *recording = false;
    println!("Stopped recording!");
}

fn main() {
    let mut libinput = unsafe { libinput_from_udev() };
    let mut uinput = uinput::UInput::new();
    let mut event_store = Vec::new();

    let options = parse_args();

    let mut recording = false;
    let mut record_start_time = time::Timespec::new(0, 0);

    println!("Swan-ag ready! Use ESC to record and F2 to replay.");

    if let Some(duration) = options.record_delay {
        sleep_secs(duration);
        start_recording(&mut event_store, &mut recording, &mut record_start_time);
    }

    loop {
        if let Err(_) = libinput.dispatch() {
            panic!("libinput dispatch failed.");
        }
        // This dispatch doesn't block and causes a busy loop. For now lets just sleep.
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Multiple events may be processed before another time check
        if let Some(duration) = options.record_length {
            if recording && time_has_elapsed(record_start_time, duration) {
                stop_recording(&mut recording);
            }
        }

        while let Some(event) = libinput.next() {
            match event {
                Keyboard(Key(key_event)) => {
                    let key = key_event.key();
                    let key_state = key_event.key_state();

                    match uinput::Key::from(key as u8) {
                        RECORD_KEY => {
                            if key_state == KeyState::Released {
                                if recording {
                                    stop_recording(&mut recording);
                                } else {
                                    start_recording(&mut event_store, &mut recording, &mut record_start_time);
                                }
                            }
                        },
                        REPLAY_KEY => {
                            if key_state == KeyState::Released {
                                if recording {
                                    stop_recording(&mut recording);
                                }

                                libinput.suspend();
                                replay_events(&options, &event_store, &mut uinput);
                                if libinput.resume().is_err() {
                                    panic!("Failed to resume libinput");
                                }
                            }
                        },
                        _ => if recording {
                            event_store.push(Keyboard(Key(key_event)));
                        },
                    }
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
