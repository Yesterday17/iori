use clap::Parser;
use clap_handler::handler;

use crate::inspect::{self, inspectors::ShortLinkInspector, Inspect};

#[derive(Parser, Clone, Default)]
#[clap(name = "inspect", short_flag = 'S')]
pub struct InspectCommand {
    url: String,
}

#[handler(InspectCommand)]
async fn handle_inspect(args: InspectCommand) -> anyhow::Result<()> {
    let inspectors: Vec<Box<dyn Inspect>> = vec![Box::new(ShortLinkInspector)];
    let (matched_inspector, data) = inspect::inspect(&args.url, inspectors).await?;
    eprintln!("{matched_inspector}: {data:?}");

    Ok(())
}
