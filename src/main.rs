extern crate ctrlc;

use clap::{ArgAction, Parser};
use std::fmt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MAX_RETRIES: usize = 10;

#[derive(Parser)]
struct Options {
    #[arg(long, short = 'a', default_value_t = false, action=ArgAction::SetTrue)]
    share_app: bool,

    #[arg(long, short = 'm', default_value_t = false, action=ArgAction::SetTrue)]
    modprobe: bool,

    #[arg(long, short = 'u', default_value_t = false, action=ArgAction::SetTrue)]
    unload: bool,
}

#[derive(Clone)]
struct Args {
    share_app: bool,
    modprobe: bool,
    unload: bool,
}

struct TimeoutError;

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Max retries reached. Exiting code.")
    }
}

impl fmt::Debug for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Increase max retries or fix the relevant Command.")
    }
}

struct ModprobeError;

impl fmt::Display for ModprobeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Could not load modules. Are they installed?")
    }
}

impl fmt::Debug for ModprobeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Likely nothing you can do here.")
    }
}

struct RmmodError;

impl fmt::Display for RmmodError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Could not unload module.")
    }
}

impl fmt::Debug for RmmodError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Could not unload module. Process closed too fast?")
    }
}

fn select_virtual_device(max_retries_var: usize) -> Result<String, TimeoutError> {
    if max_retries_var == 0 {
        return Err(TimeoutError);
    }
    let output = Command::new("v4l2-ctl")
        .arg("--list-devices")
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute v4l2-ctl.");

    let output_string = String::from_utf8_lossy(&output.stdout);

    let lines: Vec<&str> = output_string.split("\n\n").collect();

    let mut newest_device: Option<&str> = None;
    for line in lines {
        if line.contains("VirtualVideoDevice") {
            newest_device = Some(line.trim().split_whitespace().last().unwrap_or(""));
            break;
        } else {
            return select_virtual_device(max_retries_var - 1);
        }
    }

    match newest_device {
        Some(device) => println!("Newest virtual device: {}", device),
        None => println!("No virtual device found"),
    }

    let file = String::from(newest_device.unwrap());
    let file_arg = String::from("--file=") + &file;
    return Ok(file_arg);
}

fn modprobe() -> Result<(), ModprobeError> {
    let mut modprobe_process = Command::new("sudo")
        .arg("modprobe")
        .arg("v4l2loopback")
        .arg("exclusive_caps=1")
        .arg("card_label=VirtualVideoDevice")
        .spawn()
        .expect("Failed to execute modprobe with sudo.");

    let modprobe_result = modprobe_process
        .wait()
        .expect("Failed to wait for modprobe process.");

    if !modprobe_result.success() {
        eprintln!(
            "modprobe failed with exit code {:?}",
            modprobe_result.code()
        );
        return Err(ModprobeError);
    } else {
        return Ok(());
    }
}

fn unload_module() -> Result<(), RmmodError> {
    let mut rmmod_process = Command::new("sudo")
        .arg("rmmod")
        .arg("v4l2loopback")
        .spawn()
        .expect("Failed to execute rmmod with sudo.");

    let rmmod_result = rmmod_process.wait().expect("Failed to wait.");

    if !rmmod_result.success() {
        eprintln!("Rmmode failed with exit code {:?}", rmmod_result.code());
        return Err(RmmodError);
    } else {
        return Ok(());
    }
}

fn main() {
    let options: Options = Options::parse();
    let args = Args {
        share_app: options.share_app,
        modprobe: options.modprobe,
        unload: options.unload,
    };

    let unload_requested = Arc::new(AtomicBool::new(args.unload));

    ctrlc::set_handler(move || {
        if unload_requested.load(Ordering::SeqCst) {
            let unload_module_result = unload_module();
            match unload_module_result {
                Err(error) => eprintln!("Error: {:?}", error),
                Ok(_) => eprintln!("Successfully unloaded module."),
            }
        }
        std::process::exit(0); // Exit the application after handling the signal
    })
    .expect("Error setting Ctrl-C handler");

    if args.modprobe == true {
        let modprobe_result = modprobe();

        match modprobe_result {
            Ok(_) => {
                eprintln!("Successfully ran modprobe command.")
            }
            Err(error) => {
                eprintln!("An error has occured: {}", error);
            }
        }
    };

    let file_arg_result = select_virtual_device(MAX_RETRIES);

    let mut file_arg = String::new();

    match file_arg_result {
        Ok(file_arg_string) => {
            file_arg = file_arg_string;
        }
        Err(error) => {
            eprintln!("Error: {}", error);
        }
    }

    if args.share_app == false {
        let mut recorder_result = Command::new("wf-recorder")
            .arg("--muxer=v4l2")
            .arg("--codec=rawvideo")
            .arg(&file_arg)
            .arg("-x")
            .arg("yuv420p")
            .spawn()
            .expect("Error, likely selected wrong device.");

        let result_output = recorder_result
            .wait()
            .expect("Failed to wait for wf-recorder.");

        if !result_output.success() {
            println!("Failed to execute wf-recorder.");
        }
    } else {
        let slurp_output = Command::new("slurp")
            .stdout(Stdio::piped())
            .output()
            .expect("Failed to execute slurp. Is it installed?");

        if !slurp_output.status.success() {
            eprintln!(
                "Slurp failed with exit code {:?}",
                slurp_output.status.code()
            );
            return;
        }

        let slurp_coordinates = String::from_utf8_lossy(&slurp_output.stdout);
        let slurp_coordinates = slurp_coordinates.trim();

        let mut recorder_result = Command::new("wf-recorder")
            .arg("-g")
            .arg(slurp_coordinates)
            .arg("--muxer=v4l2")
            .arg("--codec=rawvideo")
            .arg(&file_arg)
            .arg("-x")
            .arg("yuv420p")
            .spawn()
            .expect("Error, do you have slurp installed?");

        let result_output = recorder_result.wait().expect("Failed to wait for output.");

        if !result_output.success() {
            println!("Failed to execute wf-recorder and slurp.");
        }
    }
}
