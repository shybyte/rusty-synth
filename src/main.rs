#[macro_use]
extern crate chan;
extern crate sdl2;
extern crate portmidi as pm;
extern crate chan_signal;

use std::f32;
use chan_signal::Signal;
use std::thread;
use std::sync::{Arc, Mutex};
use pm::{PortMidi};


use sdl2::audio::{AudioCallback, AudioSpecDesired};
use std::time::Duration;

struct SquareWave {
    freq: Arc<Mutex<f32>>,
    phase: f32,
    volume: f32,
    spec_freq: f32,
}

impl AudioCallback for SquareWave {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // Generate a square wave
        let phase_inc: f32 = (*self.freq.lock().unwrap()) / self.spec_freq;
        for x in out.iter_mut() {
            *x = match self.phase {
                0.0 ... 0.5 => self.volume,
                _ => -self.volume
            };
            self.phase = (self.phase + phase_inc) % 1.0;
        }
    }
}


fn print_devices(pm: &PortMidi) {
    for dev in pm.devices().unwrap() {
        println!("{}", dev);
    }
}

const BUF_LEN: usize = 1024;

fn main() {
    println!("Starting Rusty Synth...");

    let context = pm::PortMidi::new().unwrap();
    print_devices(&context);

    let in_devices: Vec<pm::DeviceInfo> = context.devices()
        .unwrap()
        .into_iter()
        .filter(|dev| dev.is_input())
        .collect();
    let in_ports: Vec<pm::InputPort> = in_devices.into_iter()
        .filter_map(|dev| {
            context.input_port(dev, BUF_LEN)
                .ok()
        })
        .collect();


    let os_signal = chan_signal::notify(&[Signal::INT, Signal::TERM]);

    let (tx, rx) = chan::sync(0);
    thread::spawn(move || {
        let timeout = Duration::from_millis(10);
        loop {
            for port in &in_ports {
                if let Ok(Some(events)) = port.read_n(BUF_LEN) {
                    tx.send((port.device(), events));
                }
            }
            thread::sleep(timeout);
        }
    });


    let sdl_context = sdl2::init().unwrap();
    let audio_subsystem = sdl_context.audio().unwrap();

    let desired_spec = AudioSpecDesired {
        freq: Some(44100),
        channels: Some(1),
        // mono
        samples: None       // default sample size
    };

    let freq = Arc::new(Mutex::new(440.0));

    let freq_clone = freq.clone();

    let device = audio_subsystem.open_playback(None, &desired_spec, |spec| {
        // Show obtained AudioSpec
        println!("{:?}", spec);

        SquareWave {
            freq: freq_clone,
            spec_freq: spec.freq as f32,
            phase: 0.0,
            volume: 0.25
        }
    }).unwrap();

    // Start playback
    device.resume();

    loop {
        chan_select! {
            rx.recv() -> midi_events => {
                let (_device, events) = midi_events.unwrap();
                for event in events {
                    match event.message.status {
                        248 => continue,
                        192 => {
                            println!("program change {:?}", event.message);
                        },
                        144 => {
                            let mut freq_pointer= freq.lock().unwrap();
                            let midi_note = event.message.data1;
                            let f =  (2.0 as f32).powf((midi_note as f32 - 57.0) / 12.0) * 220.0;
                            *freq_pointer = f;
                        }
                        _ => {
                            println!("event = {:?}", event);
                        }
                    }
                }
            },
            os_signal.recv() -> os_sig => {
                println!("received os signal: {:?}", os_sig);
                if os_sig == Some(Signal::INT) {
                    break;
                }
            }
        }
    }
}
