use std::collections::HashMap;

use crate::inspect::{
    self,
    inspectors::{ExternalInspector, HlsInspector, ShortLinkInspector},
    Inspect, InspectExt,
};
use clap::Parser;
use clap_handler::handler;
use iori_nicolive::inspect::NicoLiveInspector;
use iori_showroom::inspect::ShowroomInspector;

#[derive(Parser, Clone, Default)]
#[clap(name = "inspect", short_flag = 'S')]
pub struct InspectCommand {
    #[clap(short, long)]
    wait: bool,

    /// Additional arguments passed to inspectors.
    ///
    /// Format: key=value
    #[clap(short = 'e', long = "inspector-arg")]
    inspector_args: Vec<String>,

    url: String,
}

pub(crate) fn get_default_external_inspector(
    input: &[String],
) -> anyhow::Result<Vec<Box<dyn Inspect>>> {
    let args: HashMap<String, String> = input
        .into_iter()
        .map(|s| {
            let (key, value) = s.split_once('=').unwrap();
            (key.to_string(), value.to_string())
        })
        .collect();

    let mut inspectors: Vec<Box<dyn Inspect>> = vec![
        ShortLinkInspector.to_box(),
        ShowroomInspector.to_box(),
        NicoLiveInspector::new(args.get("nico_user_session").cloned()).to_box(),
        HlsInspector.to_box(),
    ];

    if let Ok(key) = std::env::var("SHIORI_EXTERNAL_INSPECTOR") {
        inspectors.push(ExternalInspector::new(&key)?.to_box());
    }

    Ok(inspectors)
}

#[handler(InspectCommand)]
async fn handle_inspect(this: InspectCommand) -> anyhow::Result<()> {
    let inspectors = get_default_external_inspector(&this.inspector_args)?;
    let (matched_inspector, data) = inspect::inspect(
        &this.url,
        inspectors,
        |c| c.into_iter().next().unwrap(),
        this.wait,
    )
    .await?;
    eprintln!("{matched_inspector}: {data:?}");

    Ok(())
}
