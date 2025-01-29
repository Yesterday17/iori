use clap::Parser;
use iori_minyami::MinyamiArgs;
use pretty_env_logger::env_logger::Builder;

/// Logger modified from pretty-env-logger
///
/// Copyright (c) 2017 Sean McArthur
///
/// Permission is hereby granted, free of charge, to any person obtaining a copy
/// of this software and associated documentation files (the "Software"), to deal
/// in the Software without restriction, including without limitation the rights
/// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
/// copies of the Software, and to permit persons to whom the Software is
/// furnished to do so, subject to the following conditions:
///
/// The above copyright notice and this permission notice shall be included in all
/// copies or substantial portions of the Software.
///
/// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
/// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
/// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
/// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
/// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
/// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
/// SOFTWARE.
pub(crate) fn logger() -> Builder {
    use std::{
        fmt,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Padded<T> {
        value: T,
        width: usize,
    }

    impl<T: fmt::Display> fmt::Display for Padded<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{: <width$}", self.value, width = self.width)
        }
    }

    static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);

    fn max_target_width(target: &str) -> usize {
        let max_width = MAX_MODULE_WIDTH.load(Ordering::Relaxed);
        if max_width < target.len() {
            MAX_MODULE_WIDTH.store(target.len(), Ordering::Relaxed);
            target.len()
        } else {
            max_width
        }
    }

    let instance_id = std::env::var("INSTANCE_ID")
        .map(|i| format!("{i} "))
        .unwrap_or_default();
    let mut builder = Builder::new();

    builder
        .format(move |f, record| {
            use pretty_env_logger::env_logger::fmt::Color;
            use std::io::Write;

            let target = record.target();
            let max_width = max_target_width(target);

            let mut style = f.style();
            let color = match record.level() {
                log::Level::Trace => Color::Magenta,
                log::Level::Debug => Color::Blue,
                log::Level::Info => Color::Green,
                log::Level::Warn => Color::Yellow,
                log::Level::Error => Color::Red,
            };
            let level = style.set_color(color).value(record.level());

            let mut style = f.style();
            let target = style.set_bold(true).value(Padded {
                value: target,
                width: max_width,
            });

            writeln!(f, " {level} {instance_id}{target} > {}", record.args())
        })
        .filter_level(log::LevelFilter::Info)
        .parse_default_env();

    builder
}

#[tokio::main(worker_threads = 8)]
async fn main() -> anyhow::Result<()> {
    logger().init();

    MinyamiArgs::parse().run().await?;

    Ok(())
}
