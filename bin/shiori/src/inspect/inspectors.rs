mod redirect;
pub use redirect::ShortLinkInspector;

mod external;
pub use external::ExternalInspector;

mod hls;
pub use hls::HlsInspector;

mod plugin;
pub use plugin::ExtismInspector;
