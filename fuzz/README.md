# Fuzzing

Install `cargo-fuzz` and run `cargo fuzz run patch` from this directory. The harness explores arbitrary JSON documents and RFC 6902 patch sequences through the same upstream patch engine used by the transaction layer.

Fuzzing complements deterministic protocol and crash tests; it does not establish filesystem durability.

