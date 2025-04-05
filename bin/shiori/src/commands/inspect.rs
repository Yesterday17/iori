use crate::inspect::{
    inspectors::{HlsInspector, ShortLinkInspector},
    Inspectors,
};
use clap::Parser;
use clap_handler::handler;
use iori_nicolive::inspect::NicoLiveInspector;
use iori_showroom::inspect::ShowroomInspector;
use shiori_plugin::InspectorArgs;

#[derive(Parser, Clone, Default)]
#[clap(name = "inspect", short_flag = 'S')]
pub struct InspectCommand {
    #[clap(short, long)]
    wait: bool,

    /// Additional arguments passed to inspectors.
    ///
    /// Format: key=value
    #[clap(short = 'e', long = "arg")]
    inspector_args: Vec<String>,

    url: String,
}

pub(crate) fn get_default_external_inspector() -> anyhow::Result<Inspectors> {
    let mut inspector = Inspectors::new();
    inspector
        .add(ShortLinkInspector)
        .add(ShowroomInspector)
        .add(NicoLiveInspector)
        .add(HlsInspector);

    // if let Ok(key) = std::env::var("SHIORI_EXTERNAL_INSPECTOR") {
    //     inspectors.push(ExternalInspector::new(&key)?.to_box());
    // }

    Ok(inspector)
}

#[handler(InspectCommand)]
async fn handle_inspect(this: InspectCommand) -> anyhow::Result<()> {
    let args = InspectorArgs::from_key_value(&this.inspector_args);
    let (matched_inspector, data) = get_default_external_inspector()?
        .wait(this.wait)
        .inspect(&this.url, args, |c| c.into_iter().next().unwrap())
        .await?;

    eprintln!("{matched_inspector}: {data:?}");

    Ok(())
}
