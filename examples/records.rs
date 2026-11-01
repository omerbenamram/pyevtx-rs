//! Prints the records of an EVTX file as evtx's own record iterators give them, one JSON object
//! per line: `{"event_record_id", "timestamp", "data", "recovered"}` for a record and
//! `{"error"}` for an error. `tests/test_rust_parity.py` checks `PyEvtxParser` against it.
//!
//! ```sh
//! cargo run --release --example records --no-default-features --features wevt_templates -- \
//!     <file> <xml|json> [recover]
//! ```

use std::io::Write;

use evtx_rs::{EvtxParser, ParserSettings};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, format, rest @ ..] = &args[..] else {
        panic!("usage: records <file> <xml|json> [recover]");
    };
    let recover = rest.iter().any(|arg| arg == "recover");
    let settings = ParserSettings::new().recover_records(recover);
    let mut parser = EvtxParser::from_path(path)
        .expect("the log opens")
        .with_configuration(settings);
    let records: Box<dyn Iterator<Item = _>> = match format.as_str() {
        "xml" => Box::new(parser.records()),
        "json" => Box::new(parser.records_json()),
        _ => panic!("the format is xml or json, not {}", format),
    };
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for record in records {
        let line = match record {
            Ok(record) => serde_json::json!({
                "event_record_id": record.event_record_id,
                "timestamp": record.timestamp.to_string(),
                "data": record.data,
                "recovered": record.recovered,
            }),
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };
        writeln!(out, "{line}").expect("stdout takes the records");
    }
    out.flush().expect("stdout takes the records");
}
