use std::{
    ffi::CString,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput};

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

        for input_context in &input_contexts {
            for input_stream in input_context.streams() {
                let mut output_stream = output_format_context.new_stream();
                output_stream.set_codecpar(input_stream.codecpar().clone());
            }
        }

        // output_format_context.dump(0, output.as_path())?;
        output_format_context.write_header(&mut None)?;

        let mut stream_index_offset = 0;
        for mut input_context in input_contexts {
            let stream_count = input_context.streams().len();
            while let Some(mut packet) = input_context.read_packet()? {
                let input_stream_index = packet.stream_index as usize;
                let output_stream_index = stream_index_offset + input_stream_index;

                {
                    let output_stream = &output_format_context.streams()[output_stream_index];
                    let input_stream = &input_context.streams()[input_stream_index];

                    packet.rescale_ts(input_stream.time_base, output_stream.time_base);
                    packet.set_stream_index(output_stream_index as i32);
                    packet.set_pos(-1);
                }

                output_format_context.interleaved_write_frame(&mut packet)?;
            }
            stream_index_offset += stream_count;
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
