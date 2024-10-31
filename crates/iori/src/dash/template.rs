// References:
// 1. https://github.com/clitic/vsd/blob/30ca1985e4a467ea3304b11c08d3176deaafd22a/vsd/src/dash/template.rs
// 2. https://github.com/emarsden/dash-mpd-rs/blob/6ebdfb4759adbda8233b5b3520804e23ff86e7de/src/fetch.rs#L435-L466

use regex::{Regex, Replacer};
use std::{collections::HashMap, sync::LazyLock};

// From https://dashif.org/docs/DASH-IF-IOP-v4.3.pdf:
// "For the avoidance of doubt, only %0[width]d is permitted and no other identifiers. The reason
// is that such a string replacement can be easily implemented without requiring a specific library."
//
// Instead of pulling in C printf() or a reimplementation such as the printf_compat crate, we reimplement
// this functionality directly.
//
// Example template: "$RepresentationID$/$Number%06d$.m4s"
static TEMPLATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$(RepresentationID|Number|Time|Bandwidth)(?:%0([\d])d)?\$").unwrap()
});

pub struct Template<'a> {
    args: HashMap<&'a str, String>,
}

impl Template<'_> {
    pub const REPRESENTATION_ID: &'static str = "RepresentationID";
    pub const NUMBER: &'static str = "Number";
    pub const TIME: &'static str = "Time";
    pub const BANDWIDTH: &'static str = "Bandwidth";

    pub fn new() -> Self {
        Self {
            args: HashMap::with_capacity(4),
        }
    }

    pub fn insert(&mut self, key: &'static str, value: String) {
        self.args.insert(key, value);
    }

    pub fn resolve(&self, template: &str) -> String {
        TEMPLATE_REGEX
            .replace_all(template, TemplateReplacer(&self.args))
            .to_string()
    }
}

struct TemplateReplacer<'a>(&'a HashMap<&'a str, String>);

impl Replacer for TemplateReplacer<'_> {
    fn replace_append(&mut self, caps: &regex::Captures<'_>, dst: &mut String) {
        let key = caps.get(1).unwrap().as_str();
        let Some(value) = self.0.get(key) else {
            dst.push_str(&caps.get(0).unwrap().as_str());
            return;
        };

        let width = caps.get(2).map(|m| m.as_str().parse().unwrap());
        if let Some(width) = width {
            dst.push_str(&format!("{value:0>width$}", width = width));
        } else {
            return dst.push_str(value.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dash::template::Template;

    #[test]
    fn test_template_replace() {
        let mut template = Template::new();
        template.insert("RepresentationID", "1".to_string());
        template.insert("Number", "2".to_string());
        template.insert("Time", "3".to_string());
        template.insert("Bandwidth", "4".to_string());

        // Single digit
        assert_eq!(template.resolve("$RepresentationID$"), "1".to_string());
        assert_eq!(template.resolve("$Number$"), "2".to_string());
        assert_eq!(template.resolve("$Time$"), "3".to_string());
        assert_eq!(template.resolve("$Bandwidth$"), "4".to_string());

        // Double digit
        assert_eq!(template.resolve("$RepresentationID%02d$"), "01".to_string());
        assert_eq!(template.resolve("$Number%02d$"), "02".to_string());
        assert_eq!(template.resolve("$Time%02d$"), "03".to_string());
        assert_eq!(template.resolve("$Bandwidth%02d$"), "04".to_string());

        // Mixed variables
        assert_eq!(
            template.resolve("$RepresentationID$-$Number$"),
            "1-2".to_string()
        );
        assert_eq!(template.resolve("$Time$-$Bandwidth$"), "3-4".to_string());

        // Mixed variables with width
        assert_eq!(
            template.resolve("$RepresentationID%02d$-$Number%09d$"),
            "01-000000002".to_string()
        );

        // All variables
        assert_eq!(
            template.resolve("$RepresentationID$-$Number$-$Time$-$Bandwidth$"),
            "1-2-3-4".to_string()
        );

        // All variables with different width
        assert_eq!(
            template.resolve("$RepresentationID%02d$-$Number%09d$-$Time%02d$-$Bandwidth%02d$"),
            "01-000000002-03-04".to_string()
        );

        // Unknown variable
        assert_eq!(template.resolve("$Unknown$"), "$Unknown$".to_string());
    }

    #[test]
    fn test_template_variable_not_defined() {
        let template = Template::new();
        assert_eq!(
            template.resolve("$RepresentationID$"),
            "$RepresentationID$".to_string()
        );
    }
}
