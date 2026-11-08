# Task 5 report: test codec as a media plugin

Implemented: DECODER_ID/DECODER_NAME, TestCodec: MediaProvider (rejects any other decoder with MediaError::Codec), registered(), install() via set_registry, manifest protocol 2 / roles ["media"] / media.decoders, format::sniff answers Unsupported for unrecognized files. Decodes (video/audio) turn that into a plain Codec error, since "unsupported" is valid only for probe.
Tests: tests ported to the decoder-first signatures; plugin_folder uses InstalledPlugins::media(); process.rs adds a_foreign_file_is_unsupported (round trip over the wire through the program, plus in process) and another_decoder_is_not_the_test_codec.
Checks: cargo test -p celesta-codec-test (10+2+6 pass), cargo test -p celesta-media (83 pass), clippy -D warnings clean, fmt check clean.
Note: main.rs unchanged in code (serve() already declares ROLES = ["media"]).
