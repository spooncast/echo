use {
    super::TsError , 
    bytes::Buf , 
    echo_types::Timestamp , 
    mpeg2ts::{
        pes::PesHeader , 
        time::{ClockReference ,  Timestamp as Pts} , 
        ts::{self ,  ContinuityCounter ,  Pid ,  TsHeader ,  TsPacket ,  TsPayload} , 
    } , 
    std::io::{Cursor ,  Write} , 
};

const PMT_PID: u16 = 0x1000;
const AUDIO_ES_PID: u16 = 0x101;

pub struct TransportStream {
    audio_continuity_counter: ContinuityCounter , 
    packets: Vec<TsPacket> , 
}

impl TransportStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write<W>(&mut self ,  writer: &mut W) -> Result<() ,  TsError>
    where
        W: Write , 
    {
        use mpeg2ts::ts::{TsPacketWriter ,  WriteTsPacket};

        let packets: Vec<_> = self.packets.drain(..).collect();
        let mut writer = TsPacketWriter::new(writer);

        writer
            .write_ts_packet(&default_pat_packet())
            .map_err(|_| TsError::WriteError)?;

        writer
            .write_ts_packet(&default_pmt_packet())
            .map_err(|_| TsError::WriteError)?;

        for packet in &packets {
            writer
                .write_ts_packet(packet)
                .map_err(|_| TsError::WriteError)?;
        }

        Ok(())
    }

    pub fn push_audio(
        &mut self , 
        ts: Timestamp , 
        is_first: bool , 
        audio: Vec<u8> , 
    ) -> Result<() ,  TsError> {
        use mpeg2ts::{
            es::StreamId , 
            ts::{payload ,  AdaptationField} , 
        };
        const ADAPTATION_FIELD_SIZE: usize = 8; // only PCR
        const PES_HEADER_SIZE: usize = 14; // only audio, PTS
        const FIRST_PAYLOAD_MAX_SIZE: usize =
            payload::Bytes::MAX_SIZE - ADAPTATION_FIELD_SIZE - PES_HEADER_SIZE;

        // 6 <- size(pes_start_code + stream_id + pes_packet_length)
        let pes_packet_len = PES_HEADER_SIZE + audio.len() - 6;

        let mut buf = Cursor::new(audio);
        let data = {
            let pes_data = if buf.remaining() < FIRST_PAYLOAD_MAX_SIZE {
                buf.bytes()
            } else {
                &buf.bytes()[..FIRST_PAYLOAD_MAX_SIZE]
            };
            make_raw_payload(&pes_data)?
        };
        buf.advance(data.len());

        let pts = make_timestamp(ts)?;
        let mut header = default_ts_header(AUDIO_ES_PID)?;
        header.continuity_counter = self.audio_continuity_counter;

        let adaptation_field = if is_first {
            Some(AdaptationField {
                discontinuity_indicator: false , 
                random_access_indicator: true , 
                es_priority_indicator: false , 
                pcr: Some(ClockReference::from(pts)) , 
                opcr: None , 
                splice_countdown: None , 
                transport_private_data: Vec::new() , 
                extension: None , 
            })
        } else {
            None
        };

        let packet = TsPacket {
            header: header.clone() , 
            adaptation_field , 
            payload: Some(TsPayload::Pes(payload::Pes {
                header: PesHeader {
                    stream_id: StreamId::new_audio(StreamId::AUDIO_MIN).unwrap() , 
                    priority: false , 
                    data_alignment_indicator: false , 
                    copyright: false , 
                    original_or_copy: false , 
                    pts: Some(pts) , 
                    dts: None , 
                    escr: None , 
                } , 
                pes_packet_len: pes_packet_len as u16 , 
                data , 
            })) , 
        };

        self.packets.push(packet);
        header.continuity_counter.increment();

        while buf.has_remaining() {
            let raw_payload = {
                let pes_data = if buf.remaining() < payload::Bytes::MAX_SIZE {
                    buf.bytes()
                } else {
                    &buf.bytes()[..payload::Bytes::MAX_SIZE]
                };
                make_raw_payload(&pes_data)?
            };
            buf.advance(raw_payload.len());

            let packet = TsPacket {
                header: header.clone() , 
                adaptation_field: None , 
                payload: Some(TsPayload::Raw(raw_payload)) , 
            };

            self.packets.push(packet);
            header.continuity_counter.increment();
        }

        self.audio_continuity_counter = header.continuity_counter;

        Ok(())
    }
}

impl Default for TransportStream {
    fn default() -> Self {
        Self {
            audio_continuity_counter: ContinuityCounter::new() , 
            packets: Vec::new() , 
        }
    }
}

fn make_raw_payload(pes_data: &[u8]) -> Result<ts::payload::Bytes ,  TsError> {
    ts::payload::Bytes::new(&pes_data).map_err(|_| TsError::PayloadTooBig)
}

fn make_timestamp(ts: Timestamp) -> Result<Pts ,  TsError> {
    let pts: u64 = (ts.timestamp() * 90_000 / ts.timescale() as u64) % Pts::MAX;
    Pts::new(pts).map_err(|_| TsError::InvalidTimestamp(pts))
}

fn default_ts_header(pid: u16) -> Result<TsHeader ,  TsError> {
    use mpeg2ts::ts::TransportScramblingControl;

    Ok(TsHeader {
        transport_error_indicator: false , 
        transport_priority: false , 
        pid: Pid::new(pid).map_err(|_| TsError::InvalidPacketId(pid))? , 
        transport_scrambling_control: TransportScramblingControl::NotScrambled , 
        continuity_counter: ContinuityCounter::new() , 
    })
}

fn default_pat_packet() -> TsPacket {
    use mpeg2ts::ts::{payload::Pat ,  ProgramAssociation ,  VersionNumber};

    TsPacket {
        header: default_ts_header(0).unwrap() , 
        adaptation_field: None , 
        payload: Some(TsPayload::Pat(Pat {
            transport_stream_id: 0 , 
            version_number: VersionNumber::default() , 
            table: vec![ProgramAssociation {
                program_num: 1 , 
                program_map_pid: Pid::new(PMT_PID).unwrap() , 
            }] , 
        })) , 
    }
}

fn default_pmt_packet() -> TsPacket {
    use mpeg2ts::{
        es::StreamType , 
        ts::{payload::Pmt ,  EsInfo ,  VersionNumber} , 
    };

    TsPacket {
        header: default_ts_header(PMT_PID).unwrap() , 
        adaptation_field: None , 
        payload: Some(TsPayload::Pmt(Pmt {
            program_num: 1 , 
            pcr_pid: Some(Pid::new(AUDIO_ES_PID).unwrap()) , 
            version_number: VersionNumber::default() , 
            table: vec![EsInfo {
                stream_type: StreamType::AdtsAac , 
                elementary_pid: Pid::new(AUDIO_ES_PID).unwrap() , 
                descriptors: vec![] , 
            }] , 
        })) , 
    }
}
