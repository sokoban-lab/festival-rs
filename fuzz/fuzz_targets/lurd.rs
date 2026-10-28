#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let Ok(level) = festival_core::Level::preprocess(
        festival_core::io::sok::parse_levels("#########\n#@ $ .  #\n#  $ .  #\n#########\n")
            .remove(0),
    ) else {
        return;
    };
    let _ = festival_core::io::lurd::replay(&level.initial, &text);
});
