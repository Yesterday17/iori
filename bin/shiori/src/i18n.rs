use std::sync::LazyLock;

use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader},
    DesktopLanguageRequester, LanguageLoader,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n"]
struct Localizations;

fn init_i18n() -> FluentLanguageLoader {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    let mut references = DesktopLanguageRequester::requested_languages();
    references.push(loader.fallback_language().clone());
    loader
        .load_languages(&Localizations, &references)
        .expect("Failed to load localization.");
    loader
}

pub static LOCALIZATION_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| init_i18n());

#[macro_export]
macro_rules! fl {
    ($message_id: literal) => {
        i18n_embed_fl::fl!(crate::i18n::LOCALIZATION_LOADER, $message_id)
    };

    ($message_id: literal, $($args: expr),*) => {
        i18n_embed_fl::fl!(crate::i18n::LOCALIZATION_LOADER, $message_id, $($args), *)
    };
}

#[macro_export]
macro_rules! ll {
    ($message_id: literal) => {
        Box::leak(crate::fl!($message_id).into_boxed_str()) as &'static str
    };
}

#[macro_export]
macro_rules! ball {
    ($message_id: literal) => {
        bail!(crate::fl!($message_id))
    };

    ($message_id: literal, $($args: expr),*) => {
        bail!(crate::fl!($message_id, $($args), *))
    };
}

pub trait ClapI18n {
    fn about_ll(self, key: &'static str) -> Self;
}

impl ClapI18n for clap::Command {
    fn about_ll(self, key: &'static str) -> Self {
        self.about(Box::leak(LOCALIZATION_LOADER.get(key).into_boxed_str()) as &'static str)
    }
}

impl ClapI18n for clap::Arg {
    fn about_ll(self, key: &'static str) -> Self {
        self.help(Box::leak(LOCALIZATION_LOADER.get(key).into_boxed_str()) as &'static str)
    }
}
