#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let raws = festival_core::io::sok::parse_levels(&text);
    for raw in raws {
        if raw.board.is_some() {
            let _ = festival_core::Level::preprocess(raw);
        }
    }
});
