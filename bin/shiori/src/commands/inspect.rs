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

    url: String,
}

pub(crate) fn get_default_external_inspector() -> anyhow::Result<Vec<Box<dyn Inspect>>> {
    let mut inspectors: Vec<Box<dyn Inspect>> = vec![
        ShortLinkInspector.to_box(),
        ShowroomInspector.to_box(),
        NicoLiveInspector::new(std::env::var("NICO_USER_SESSION").ok()).to_box(),
        HlsInspector.to_box(),
    ];

    if let Ok(key) = std::env::var("SHIORI_EXTERNAL_INSPECTOR") {
        inspectors.push(ExternalInspector::new(&key)?.to_box());
    }

    Ok(inspectors)
}

#[handler(InspectCommand)]
async fn handle_inspect(args: InspectCommand) -> anyhow::Result<()> {
    let inspectors = get_default_external_inspector()?;
    let (matched_inspector, data) = inspect::inspect(
        &args.url,
        inspectors,
        |c| c.into_iter().next().unwrap(),
        args.wait,
    )
    .await?;
    eprintln!("{matched_inspector}: {data:?}");

    Ok(())
}
