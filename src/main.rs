use gumdrop::Options;

use std::error::Error;
use wrclip::copy;
use wrclip::paste;

#[allow(non_camel_case_types)]
#[derive(Debug, Options)]
/// Access a wayland compositor clipboard for reading or writing.
struct MyOptions {
    #[options(help = "print help message")]
    help: bool,
    #[options(command)]
    command: Option<Command>,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Options)]
enum Command {
    #[options(
        help = "Copy stdin into clipboard(input) with the MIME types given as a space separated list"
    )]
    i(MimeOpts),
    #[options(
        help = "Paste clipboard to stdout(output), trying each MIME type in the order given until a match is found. \
        If no MIME type is provided, the program will default to text"
    )]
    o(MimeOpts),
}

#[derive(Debug, Options)]
struct MimeOpts {
    #[options(help = "mime types")]
    mimes: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts: MyOptions = MyOptions::parse_args_default_or_exit();

    if let Some(command) = opts.command {
        match command {
            Command::i(MimeOpts { mimes }) => copy(get_mimes(mimes)),
            Command::o(MimeOpts { mimes }) => paste(get_mimes(mimes)),
        }?;
    }

    Ok(())
}

fn get_mimes(mimes: Vec<String>) -> Vec<String> {
    if mimes.is_empty() {
        return vec![String::from("text/plain;charset=utf-8")];
    }
    mimes
}
