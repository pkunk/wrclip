extern crate anyhow;
extern crate structopt;

use structopt::StructOpt;

use wrclip::copy;
use wrclip::paste;

#[derive(Debug, StructOpt)]
/// wrclip i <mime_types>
/// Copy stdin into clipboard(input) with the MIME types
/// given as a space separated list
/// wrclip o <mime_types>
/// Paste clipboard to stdout(output), trying each MIME type in
/// the order given until a match is found
/// If no MIME type is provided, the program will default to text
enum Cli {
    #[structopt(name = "i")]
    Input { mimes: Vec<String> },
    #[structopt(name = "o")]
    Output { mimes: Vec<String> },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::from_args();
    match cli {
        Cli::Input { mimes } => copy(get_mimes(mimes)),
        Cli::Output { mimes } => paste(get_mimes(mimes)),
    }?;
    Ok(())
}

fn get_mimes(mimes: Vec<String>) -> Vec<String> {
    if mimes.is_empty() {
        return vec![String::from("text/plain;charset=utf-8")];
    }
    mimes
}
