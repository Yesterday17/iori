mod redirect;
pub use redirect::ShortLinkInspector;

mod external;
pub use external::ExternalInspector;

mod template;

mod plugin;
pub use plugin::ExtismInspector;
