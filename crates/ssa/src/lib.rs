mod error;
pub use error::*;

use aes::cipher::{BlockDecryptMut, KeyIvInit};
use memchr::memmem;
use mpeg2ts::es::StreamType;
use mpeg2ts::pes::PesHeader;
use mpeg2ts::ts::payload::Bytes;
use mpeg2ts::ts::payload::Pes;
use mpeg2ts::ts::ContinuityCounter;
use mpeg2ts::ts::ReadTsPacket;
use mpeg2ts::ts::TsHeader;
use mpeg2ts::ts::TsPacket;
use mpeg2ts::ts::TsPacketReader;
use mpeg2ts::ts::TsPacketWriter;
use mpeg2ts::ts::TsPayload;
use mpeg2ts::ts::WriteTsPacket;
use std::collections::HashMap;
use std::io::BufWriter;
use std::io::Read;
use std::io::Write;

pub struct NALUnit {
    data: Vec<u8>,
    pub r#type: u8,
    length: usize,
    start_code_length: u8,
}

impl NALUnit {
    pub fn get_next(input: &[u8]) -> Result<(Self, &[u8])> {
        let start_code_length = if input.len() > 4 && &input[0..4] == b"\x00\x00\x00\x01" {
            4
        } else if input.len() > 3 && &input[0..3] == b"\x00\x00\x01" {
            3
        } else {
            return Err(Error::InvalidStartCode);
        };

        let next = &input[start_code_length..];
        let next_pos = if let Some(pos) = memmem::find(next, b"\x00\x00\x01") {
            // check pos - 1 for 0x00
            if pos > 0 && next[pos - 1] == 0x00 {
                start_code_length + pos - 1
            } else {
                start_code_length + pos
            }
        } else {
            input.len()
        };
        let next = &input[next_pos..];

        let data = input[start_code_length..next_pos].to_vec();
        Ok((
            Self {
                r#type: data[0] & 0x1f,
                data,
                length: next_pos - start_code_length,
                start_code_length: start_code_length as u8,
            },
            next,
        ))
    }

    fn remove_scep_3_bytes(&mut self) {
        let mut i = 0;
        let mut j = 0;

        while i < self.length {
            if self.length - i > 3 && self.data[i..i + 3] == [0x00, 0x00, 0x03] {
                self.data[j] = 0x00;
                self.data[j + 1] = 0x00;
                i += 3;
                j += 2;
            } else {
                self.data[j] = self.data[i];
                i += 1;
                j += 1;
            }
        }

        self.data.truncate(j);
        self.length = j;
    }

    /// Encrypted_nal_unit () {
    ///     nal_unit_type_byte                // 1 byte
    ///     unencrypted_leader                // 31 bytes
    ///     while (bytes_remaining() > 0) {
    ///         if (bytes_remaining() > 16) {
    ///             encrypted_block           // 16 bytes
    ///         }
    ///         unencrypted_block           // MIN(144, bytes_remaining()) bytes
    ///     }
    /// }
    pub fn decrypt(&mut self, key: &[u8; 16], iv: &[u8; 16]) {
        if self.data.len() <= 48 {
            return;
        }

        self.remove_scep_3_bytes();

        let mut decryptor = cbc::Decryptor::<aes::Aes128>::new(key.into(), iv.into());

        if self.data.len() < 32 {
            return;
        }

        let mut pos = &mut self.data.as_mut_slice()[32..];

        while pos.len() > 0 {
            if pos.len() > 16 {
                let block = &mut pos[..16];
                decryptor.decrypt_block_mut(block.into());
                pos = &mut pos[16..];
            }

            let remaining_len = pos.len();
            pos = &mut pos[144.min(remaining_len)..];
        }
    }

    pub fn write<W: Write>(&self, output: &mut W) -> Result<()> {
        if self.start_code_length == 4 {
            output.write_all(&[0x00, 0x00, 0x00, 0x01])?;
        } else {
            output.write_all(&[0x00, 0x00, 0x01])?;
        }
        output.write_all(&self.data)?;

        Ok(())
    }
}

struct AdtsHeader {
    length: usize,
    crc: bool,
}

impl AdtsHeader {
    fn new(data: &[u8]) -> Self {
        Self {
            length: Self::read_adts_frame_length(data),
            // Protection absence, set to 1 if there is no CRC and 0 if there is CRC.
            crc: data[1] & 0x01 == 0,
        }
    }

    fn data<'a, 'b>(&'a self, input: &'b mut [u8]) -> &'b mut [u8] {
        &mut input[if self.crc { 9 } else { 7 }..]
    }

    fn read_adts_frame_length(header: &[u8]) -> usize {
        // 2
        let byte3 = header[3] as u16;
        // 8
        let byte4 = header[4] as u16;
        // 3
        let byte5 = header[5] as u16;

        // Extract and combine bits
        let length = ((byte3 & 0b11) << 11) | (byte4 << 3) | (byte5 >> 5);
        length as usize
    }
}

struct PESSegment {
    stream_type: StreamType,

    header: PesHeader,
    pes_packet_len: u16,
    initial_size: usize,
    initial_ts_header: TsHeader,

    data: Vec<u8>,
}

impl PESSegment {
    fn decrypt_and_write<W: Write>(
        mut self,
        key: [u8; 16],
        iv: [u8; 16],
        writer: &mut IoriTsPacketWriter<W>,
    ) -> Result<()> {
        // do decrypt first
        match self.stream_type {
            StreamType::H264WithAes128Cbc => self.decrypt_video(key, iv)?,
            StreamType::AdtsAacWithAes128Cbc => self.decrypt_audio(key, iv),
            _ => unreachable!(),
        }

        let pid = self.initial_ts_header.pid;

        // split data into PES packets and write
        // TS packet size is 188
        let mut input = self.data.as_slice();
        let initial_size = input.len().min(self.initial_size);
        let packet = TsPacket {
            header: self.initial_ts_header,
            adaptation_field: None,
            payload: Some(mpeg2ts::ts::TsPayload::Pes(Pes {
                header: self.header,
                pes_packet_len: self.pes_packet_len,
                data: Bytes::new(&self.data[..initial_size])?,
            })),
        };
        writer.write_ts_packet(&packet)?;

        input = &input[initial_size..];
        while input.len() > 0 {
            let size = input.len().min(184);
            let data = &input[..size];
            input = &input[size..];

            let packet = TsPacket {
                header: TsHeader {
                    transport_error_indicator: false,
                    transport_priority: false,
                    pid,
                    transport_scrambling_control:
                        mpeg2ts::ts::TransportScramblingControl::ScrambledWithOddKey,
                    continuity_counter: ContinuityCounter::new(), // will be set by writer
                },
                adaptation_field: None,
                payload: Some(mpeg2ts::ts::TsPayload::Raw(Bytes::new(data).unwrap())),
            };
            writer.write_ts_packet(&packet)?;
        }

        Ok(())
    }

    fn decrypt_video(&mut self, key: [u8; 16], iv: [u8; 16]) -> Result<()> {
        let mut input = self.data.as_slice();
        let output = Vec::with_capacity(self.data.len() * 2);
        let mut output = BufWriter::new(output);

        loop {
            let (mut nal_unit, data_new) = NALUnit::get_next(input)?;
            input = data_new;

            if nal_unit.r#type == 5 || nal_unit.r#type == 1 {
                nal_unit.decrypt(&key, &iv);
            }

            nal_unit.write(&mut output)?;

            if input.len() == 0 {
                break;
            }
        }

        self.data = output.into_inner().map_err(|e| e.into_error())?;

        Ok(())
    }

    fn decrypt_audio(&mut self, key: [u8; 16], iv: [u8; 16]) {
        let mut input = self.data.as_mut_slice();
        while input.len() > 0 {
            let size = Self::decrypt_audio_sample(input, key, iv);
            input = &mut input[size..];
        }
    }

    /// Encrypted_AAC_Frame () {
    ///     ADTS_Header                        // 7 or 9 bytes
    ///     unencrypted_leader                 // 16 bytes
    ///     while (bytes_remaining() >= 16) {
    ///         encrypted_block                // 16 bytes
    ///     }
    ///     unencrypted_trailer                // 0-15 bytes
    /// }
    fn decrypt_audio_sample(input: &mut [u8], key: [u8; 16], iv: [u8; 16]) -> usize {
        let adts = AdtsHeader::new(&input);
        let data = adts.data(input);

        let mut decryptor = cbc::Decryptor::<aes::Aes128>::new(&key.into(), &iv.into());

        let mut is_first = true;
        let chunks = data.chunks_mut(16);
        for chunk in chunks {
            if chunk.len() < 16 || is_first {
                is_first = false;
                continue;
            }
            decryptor.decrypt_block_mut(chunk.into());
        }

        adts.length
    }
}

struct IoriTsPacketWriter<W> {
    inner: TsPacketWriter<W>,
    counters: HashMap<u16, ContinuityCounter>,
}

impl<W: Write> IoriTsPacketWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner: TsPacketWriter::new(inner),
            counters: HashMap::new(),
        }
    }

    fn get_counter(&mut self, pid: u16) -> &mut ContinuityCounter {
        self.counters.entry(pid).or_insert(ContinuityCounter::new())
    }
}

impl<W: Write> WriteTsPacket for IoriTsPacketWriter<W> {
    fn write_ts_packet(&mut self, packet: &TsPacket) -> mpeg2ts::Result<()> {
        // TODO: do not clone
        let mut packet = packet.clone();

        let counter = self.get_counter(packet.header.pid.as_u16());
        packet.header.continuity_counter = counter.clone();

        if !matches!(packet.payload, None | Some(TsPayload::Null(_))) {
            counter.increment();
        }

        self.inner.write_ts_packet(&packet)
    }
}

pub fn decrypt<R, W>(input: R, output: W, key: [u8; 16], iv: [u8; 16]) -> Result<()>
where
    R: Read,
    W: Write,
{
    let mut reader = TsPacketReader::new(input);
    let mut writer = IoriTsPacketWriter::new(output);

    let mut streams = HashMap::new();
    let mut pid_map = HashMap::new();

    while let Ok(Some(packet)) = reader.read_ts_packet() {
        if let Some(payload) = &packet.payload {
            let mut flush = Some(packet.header.pid);

            // print payload type
            match payload {
                TsPayload::Pat(_) => {
                    // no need to modify PAT
                    writer.write_ts_packet(&packet)?;
                }
                TsPayload::Pmt(pmt) => {
                    let mut pmt = pmt.clone();

                    // modify from encrypted to clear stream
                    for es in pmt.es_info.iter_mut() {
                        // save stream type before modify
                        pid_map.insert(es.elementary_pid.as_u16(), es.stream_type);

                        es.stream_type = match es.stream_type {
                            StreamType::H264WithAes128Cbc => StreamType::H264,
                            StreamType::AdtsAacWithAes128Cbc => StreamType::AdtsAac,
                            _ => es.stream_type,
                        };
                    }
                    writer.write_ts_packet(&TsPacket {
                        payload: Some(mpeg2ts::ts::TsPayload::Pmt(pmt)),
                        ..packet
                    })?;
                }
                TsPayload::Pes(pes) => {
                    let stream_type = pid_map
                        .get(&packet.header.pid.as_u16())
                        .expect("Unknown stream");
                    if !matches!(
                        stream_type,
                        StreamType::H264WithAes128Cbc | StreamType::AdtsAacWithAes128Cbc
                    ) {
                        log::debug!("Unmodified stream type: {:?}", stream_type);
                        // No need to modify unmodified stream
                        writer.write_ts_packet(&packet)?;
                        continue;
                    }

                    let prev_pes = streams.insert(
                        packet.header.pid,
                        PESSegment {
                            stream_type: stream_type.clone(),

                            initial_ts_header: packet.header.clone(),
                            header: pes.header.clone(),
                            pes_packet_len: pes.pes_packet_len,
                            initial_size: pes.data.len(),
                            data: pes.data.to_vec(),
                        },
                    );

                    if let Some(pes) = prev_pes {
                        pes.decrypt_and_write(key, iv, &mut writer)?;
                    }

                    flush = None;
                }
                TsPayload::Raw(bytes) => {
                    if let Some(pes) = streams.get_mut(&packet.header.pid) {
                        pes.data.extend_from_slice(bytes);
                    } else {
                        log::debug!("Unknown stream: {:?}", packet.header.pid);
                        writer.write_ts_packet(&packet)?;
                    }
                    flush = None;
                }
                TsPayload::Section(_) => writer.write_ts_packet(&packet)?,
                TsPayload::Null(_) => {
                    writer.write_ts_packet(&packet)?;
                    flush = None;
                }
            }

            if let Some(flush) = flush {
                if let Some(pes) = streams.remove(&flush) {
                    pes.decrypt_and_write(key, iv, &mut writer)?;
                };
            }
        }
    }

    // handle remaining streams
    for pes in streams.into_values() {
        pes.decrypt_and_write(key, iv, &mut writer)?;
    }

    Ok(())
}
