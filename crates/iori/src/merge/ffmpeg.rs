use std::{
    ffi::CString,
    path::{Path, PathBuf},
};

use rsmpeg::{
    avformat::{AVFormatContextInput, AVFormatContextOutput},
    ffi::{av_log_format_line2, av_log_set_callback, AV_LOG_ERROR, AV_LOG_INFO, AV_LOG_WARNING},
    UnsafeDerefMut,
};

use crate::IoriResult;

// Reference: https://github.com/YeautyYE/ez-ffmpeg/blob/a249e8ad35196cdf345e3f3dc93c87cfb263bfef/src/core/mod.rs#L434-L463
#[cfg(any(
    all(
        not(target_arch = "aarch64"),
        not(target_arch = "powerpc"),
        not(target_arch = "s390x"),
        not(target_arch = "x86_64")
    ),
    all(target_arch = "aarch64", target_vendor = "apple"),
    target_family = "wasm",
    target_os = "uefi",
    windows,
))]
type VaListType = *mut ::std::os::raw::c_char;

#[cfg(all(target_arch = "x86_64", not(target_os = "uefi"), not(windows)))]
type VaListType = *mut rsmpeg::ffi::__va_list_tag;

#[cfg(all(
    target_arch = "aarch64",
    not(target_vendor = "apple"),
    not(target_os = "uefi"),
    not(windows),
))]
pub type VaListType = *mut rsmpeg::ffi::__va_list_tag_aarch64;

#[cfg(all(target_arch = "powerpc", not(target_os = "uefi"), not(windows)))]
pub type VaListType = *mut rsmpeg::ffi::__va_list_tag_powerpc;

#[cfg(target_arch = "s390x")]
pub type VaListType = *mut rsmpeg::ffi::__va_list_tag_s390x;

unsafe extern "C" fn ffmpeg_log_callback(
    ptr: *mut ::std::os::raw::c_void,
    level: ::std::os::raw::c_int,
    fmt: *const ::std::os::raw::c_char,
    vargs: VaListType,
) {
    if level > AV_LOG_INFO as i32 {
        return;
    }

    let mut buf = [0i8; 1024];
    let mut print_prefix = 1;

    let buf_len = av_log_format_line2(
        ptr,
        level,
        fmt,
        vargs,
        buf.as_mut_ptr(),
        buf.len() as i32,
        &mut print_prefix,
    );

    if buf_len < 0 {
        tracing::error!("ffmpeg log callback error: {}", buf_len);
        return;
    }

    let data = &buf[..buf_len as usize];
    let data = unsafe { &*(data as *const _ as *const [u8]) };
    let data = String::from_utf8_lossy(data);
    let data = data.trim_end_matches(['\r', '\n', ' ']);
    if data.is_empty() {
        return;
    }

    let level = level as u32;
    if level <= AV_LOG_ERROR {
        tracing::error!("{data}");
    } else if level <= AV_LOG_WARNING {
        tracing::warn!("{data}");
    } else if level <= AV_LOG_INFO {
        tracing::info!("{data}");
    }
}

pub(crate) async fn ffmpeg_merge<O>(tracks: Vec<PathBuf>, output: O) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    unsafe {
        av_log_set_callback(Some(ffmpeg_log_callback));
    }

    let output = output.as_ref().to_path_buf();
    let c_tracks = tracks
        .iter()
        .map(|track| CString::new(track.as_os_str().as_encoded_bytes()))
        .collect::<Result<Vec<_>, _>>()?;

    tokio::task::spawn_blocking(move || -> IoriResult<()> {
        let c_output = CString::new(output.as_os_str().as_encoded_bytes())?;
        let mut output_format_context = AVFormatContextOutput::create(&c_output, None)?;

        let mut input_contexts = vec![];
        for c_track in c_tracks {
            let input_context: AVFormatContextInput =
                AVFormatContextInput::open(&c_track, None, &mut None)?;
            input_contexts.push(input_context);
        }

        // [track][stream] -> output_stream_index
        let mut total_stream_count = 0;
        let mut stream_mapping = Vec::new();
        for input_context in &input_contexts {
            let mut mapping = Vec::new();
            for input_stream in input_context.streams() {
                let codec_type = input_stream.codecpar().codec_type();
                if !codec_type.is_video() && !codec_type.is_audio() {
                    mapping.push(None);
                    continue;
                }

                let mut output_stream = output_format_context.new_stream();
                let mut codecpar = input_stream.codecpar().clone();
                {
                    let codecpar = unsafe { codecpar.deref_mut() };
                    codecpar.codec_tag = 0;
                }
                output_stream.codecpar_mut().copy(&codecpar);
                mapping.push(Some(total_stream_count));
                total_stream_count += 1;
            }
            stream_mapping.push(mapping);
        }

        output_format_context.write_header(&mut None)?;

        for (input_context, mapping) in input_contexts.iter_mut().zip(stream_mapping) {
            while let Some(mut packet) = input_context.read_packet()? {
                let input_stream_index = packet.stream_index as usize;
                let Some(output_stream_index) = mapping[input_stream_index] else {
                    continue;
                };

                {
                    let output_stream = &output_format_context.streams()[output_stream_index];
                    let input_stream = &input_context.streams()[input_stream_index];

                    packet.rescale_ts(input_stream.time_base, output_stream.time_base);
                    packet.set_stream_index(output_stream_index as i32);
                    packet.set_pos(-1);
                }

                output_format_context.interleaved_write_frame(&mut packet)?;
            }
        }

        output_format_context.write_trailer()?;
        Ok(())
    })
    .await??;

    // remove temporary files
    for track in tracks {
        tokio::fs::remove_file(track).await?;
    }

    Ok(())
}
