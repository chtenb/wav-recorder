#[macro_use]
extern crate lazy_static;

extern crate anyhow;

use std::env;

use anyhow::Result;
use clap::Parser;
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device,
};
extern crate cpal;

mod record;

/// Simple program to record wav files
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Optionally specify a different output directory than the current working directory
    #[clap(short, long, default_value("."))]
    output_dir: String,
    /// Specify the device to record from by index or by name
    #[clap(short, long, default_value("default"))]
    device: String,
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Start interactive recording session
    Start {},
    /// List the available devices
    Devices {},
}

fn main() {
    let args = Args::parse();
    resolve_output_dir(&args.output_dir);

    match run_command(args) {
        Err(e) => panic!("{}", e),
        Ok(()) => std::process::exit(0),
    };
}

fn run_command(args: Args) -> Result<()> {
    match args.command {
        Command::Start {} => {
            let device = resolve_device(&args.device);
            record::run(&device)
        }
        Command::Devices {} => list_devices(),
    }
}

pub fn resolve_output_dir(output_dir: &str) -> () {
    if output_dir != "." {
        env::set_current_dir(output_dir).expect("Could not access specified output directory.");
    }
}

pub fn resolve_device(device_name: &str) -> Device {
    let host = cpal::default_host();
    if device_name == "default" {
        host.default_input_device()
    } else if let Ok(i) = device_name.parse::<usize>() {
        // Find by index
        host.input_devices().unwrap().nth(i)
    } else {
        // Find by name
        host.input_devices()
            .unwrap()
            .find(|x| x.name().map(|y| y == device_name).unwrap_or(false))
    }
    .expect("Failed to find input device")
}

pub fn is_default_device(device_name: &str) -> bool {
    let host = cpal::default_host();
    match host.default_input_device().and_then(|dev| dev.name().ok()) {
        Some(default_device_name) => device_name == default_device_name,
        None => false,
    }
}

pub fn list_devices() -> Result<()> {
    let host = cpal::default_host();

    host.input_devices()?.enumerate().for_each(|(i, dev)| {
        match dev.name() {
            Ok(name) => println!(
                "{}: {} {}",
                i,
                &name,
                if is_default_device(&name) { "(default)" } else { "" }
            ),
            Err(err) => println!("{}", err),
        };
    });

    Ok(())
}
