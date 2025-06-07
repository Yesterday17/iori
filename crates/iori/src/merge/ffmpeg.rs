use std::{
    ffi::CString,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use rsmpeg::{
    avformat::{AVFormatContextInput, AVFormatContextOutput},
    UnsafeDerefMut,
};

use crate::IoriResult;

pub(crate) async fn ffmpeg_merge<O>(tracks: Vec<PathBuf>, output: O) -> IoriResult<()>
where
    O: AsRef<Path>,
{
    let output = output.as_ref().to_path_buf();
    let c_tracks = tracks
        .iter()
        .map(|track| CString::new(track.as_os_str().as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;

    tokio::task::spawn_blocking(move || -> IoriResult<()> {
        let c_output = CString::new(output.as_os_str().as_bytes())?;
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

        output_format_context.dump(0, &c_output)?;
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
