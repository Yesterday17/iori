#!/bin/sh
#![allow(unused_attributes)] /*
                             OUT=/tmp/tmp && rustc "$0" -o ${OUT} && exec ${OUT} $@ || exit $? #*/

use std::fs;
use std::io::Result;
use std::process::Command;

fn main() -> Result<()> {
    if fs::metadata("tmp").is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("./build/linux_ffmpeg.rs").status()?;
    }

    // #[cfg(target_os = "macos")]
    // {
    //     Command::new("./macos_ffmpeg.rs").status()?;
    // }

    Ok(())
}
