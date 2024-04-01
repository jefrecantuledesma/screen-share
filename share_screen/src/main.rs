use std::process::{Command, Stdio};

fn main() {
    let mut modprobe_process = Command::new("sudo")
        .arg("modprobe")
        .arg("v4l2loopback")
        .arg("exclusive_caps=1")
        .arg("card_label=VirtualVideoDevice")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
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
        return;
    }

    println!("Successfully created new video device.");

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
        }
    }

    match newest_device {
        Some(device) => println!("Newest virtual device: {}", device),
        None => println!("No virtual device found"),
    }

    let file = String::from(newest_device.unwrap());
    let file_arg = String::from("--file=") + &file;

    println!("{}", file_arg);

    let mut recorder_result = Command::new("wf-recorder")
        .arg("--muxer=v4l2")
        .arg("--codec=rawvideo")
        .arg(file_arg)
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
}
