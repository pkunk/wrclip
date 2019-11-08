extern crate rand;

use rand::distributions::Alphanumeric;
use rand::Rng;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn simple_copy_paste() {
    let program = "target/debug/wrclip";

    let rand_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .collect();

    let copy_clip = Command::new(program)
        .arg("i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut writer = copy_clip.stdin.unwrap();
    writer.write_all(rand_string.as_bytes()).unwrap();
    drop(writer);

    thread::sleep(Duration::from_millis(1000));

    let out = Command::new(program).arg("o").output().unwrap();

    assert_eq!(String::from_utf8(out.stdout).unwrap(), rand_string);
}
