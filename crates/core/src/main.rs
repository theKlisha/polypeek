use std::fmt::Debug;
use std::{io, marker};

use crate::registry::Registry;
use crate::transcoder::{Data, DataFormatMeta, Transcoder, TranscoderError, TranscoderResult};

pub mod registry;
pub mod transcoder;

#[derive(Default, Debug)]
struct RawData(Vec<u8>);

impl Data for RawData {
    fn data_format_meta() -> &'static DataFormatMeta {
        &DataFormatMeta {
            name: "raw",
            description: "raw bytes",
        }
    }
}

#[derive(Default, Debug)]
struct TextData(String);

impl Data for TextData {
    fn data_format_meta() -> &'static DataFormatMeta {
        &DataFormatMeta {
            name: "text",
            description: "utf8 encoded string",
        }
    }
}

#[derive(Clone)]
struct RawToText;

impl Transcoder<RawData, TextData> for RawToText {
    fn transcode(&self, input: RawData) -> TranscoderResult<TextData> {
        match String::from_utf8(input.0) {
            Ok(data) => Ok(TextData(data)),
            Err(e) => Err(TranscoderError::Other(e.to_string())),
        }
    }
}

#[derive(Clone)]
struct TextToRaw;

impl Transcoder<TextData, RawData> for TextToRaw {
    fn transcode(&self, input: TextData) -> TranscoderResult<RawData> {
        let data = input.0.bytes().collect::<Vec<u8>>();
        Ok(RawData(data))
    }
}

fn function_transcoder(input: TextData) -> TranscoderResult<RawData> {
    let data = input.0.bytes().collect::<Vec<u8>>();
    Ok(RawData(data))
}

fn main() -> io::Result<()> {
    let mut registry = Registry::default();

    let closure_transcoder = |input: TextData| -> TranscoderResult<RawData> {
        let mut data = input.0.bytes().collect::<Vec<u8>>();
        Ok(RawData(data))
    };

    registry.add_codec(RawToText);
    registry.add_codec(TextToRaw);
    registry.add_codec(function_transcoder);
    registry.add_codec(closure_transcoder);

    println!("{:#?}", registry);

    Ok(())
}
