#[macro_use]
extern crate chan;
extern crate sdl2;
extern crate portmidi as pm;
extern crate chan_signal;

use std::f32;
use chan_signal::Signal;
use std::thread;
use std::sync::mpsc;
use pm::{PortMidi};


use sdl2::audio::{AudioCallback, AudioSpecDesired};
use std::time::Duration;

#[derive(Debug)]
enum Command {
    NoteOn(f32),
    NoteOff()
}

struct SquareWave {
    commands: mpsc::Receiver<Command>,
    freq: f32,
    phase: f32,
    volume: f32,
    spec_freq: f32,
    is_on: bool
}

impl AudioCallback for SquareWave {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // Generate a square wave
        for command in self.commands.try_iter() {
            println!("command = {:?}, {:?}", command, out.len());
            match command {
                Command::NoteOn(freq) => {
                    self.freq = freq;
                    self.is_on = true;
                }
                Command::NoteOff() => {
                    self.is_on = false;
                }
            }
        }
        let phase_inc: f32 = self.freq / self.spec_freq;
        for x in out.iter_mut() {
            if !self.is_on {
                *x = 0.0;
                continue;
            }
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

    let (tx_audio, rx_audio) = mpsc::channel();

    let device = audio_subsystem.open_playback(None, &desired_spec, |spec| {
        // Show obtained AudioSpec
        println!("{:?}", spec);

        SquareWave {
            commands: rx_audio,
            freq: 220.0,
            spec_freq: spec.freq as f32,
            phase: 0.0,
            volume: 0.25,
            is_on: false
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
                        0x90 => {
                            let midi_note = event.message.data1;
                            let f =  (2.0 as f32).powf((midi_note as f32 - 57.0) / 12.0) * 220.0;
                            tx_audio.send(Command::NoteOn(f)).unwrap();
                        },
                        0x80 => {
                            tx_audio.send(Command::NoteOff()).unwrap();
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
