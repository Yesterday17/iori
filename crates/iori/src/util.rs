use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

pub mod ordered_stream;

pub fn file_name_add_suffix<T: AsRef<OsStr>>(path: &mut PathBuf, suffix: T) {
    let mut filename = OsString::new();

    // {file_stem}_{suffix}.{ext}
    if let Some(file_stem) = path.file_stem() {
        filename.push(file_stem);
    }
    filename.push("_");
    filename.push(suffix);

    if let Some(ext) = path.extension() {
        filename.push(".");
        filename.push(ext);
    }

    path.set_file_name(filename);
}
